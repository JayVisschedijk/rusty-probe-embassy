#![no_std]
#![no_main]

use bitbang_dap::{BitbangAdapter, DelayCycles, InputOutputPin};
use dap_rs::dap::{self, Dap, DapLeds, DapVersion, DelayNs};
use dap_rs::jtag::TapConfig;
use dap_rs::swo::Swo;
use defmt::{todo, warn};
use embassy_executor::Spawner;
use embassy_rp::gpio::{Flex, Level, Output, Pin};
use embassy_rp::peripherals::{PIN_0, USB};
use embassy_rp::usb::{self, Driver};
use embassy_rp::{Peri, bind_interrupts};
use embassy_usb::{
    self, Builder, UsbDevice,
    class::{
        cdc_acm::{CdcAcmClass, State},
        cmsis_dap_v2::{CmsisDapV2Class, State as CmsisDapV2State},
    },
    msos::windows_version,
};
use static_cell::{ConstStaticCell, StaticCell};

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // Configuration options:
    // 1. Pinout
    let t_nrst = p.PIN_9;
    let t_jtdi = p.PIN_17;
    let t_jtms_swdio = p.PIN_10;
    let dir_swdio = p.PIN_12;
    let t_jtck_swclk = p.PIN_11;
    let dir_swclk = p.PIN_19;
    let t_jtdo = p.PIN_16;
    //let t_swo // Not supported yet

    // 2. Max JTAG scan chain
    const MAX_SCAN_CHAIN_LENGTH: usize = 8;

    // 3. USB configuration
    const MANUFACTURER: &str = "Sarif";
    const PRODUCT: &str = "Rusty Probe with Embassy CMSIS-DAP v2";

    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, Irqs);

    // // Get unique id from flash
    // let mut flash = Flash::<_, _, 0>::new_blocking(p.FLASH);

    // let mut uid = [0; 8];
    // flash.blocking_unique_id(&mut uid).unwrap();

    // static SERIAL: ConstStaticCell<String<16>> = ConstStaticCell::new(String::<16>::new());
    // let serial = SERIAL.take();
    // for b in uid {
    //     let lower = b & 0x0F;
    //     let upper = (b >> 4) & 0x0F;
    //     fn hex(nibble: u8) -> char {
    //         if nibble < 10 {
    //             (b'0' + nibble) as char
    //         } else {
    //             (b'A' + nibble - 10) as char
    //         }
    //     }
    //     unwrap!(serial.push(hex(upper)));
    //     unwrap!(serial.push(hex(lower)));
    // }

    // todo: `flash.blocking_unique_id()` not implemented for rp235xa, using random serial number for now
    let serial = "012345";

    // Create embassy-usb Config
    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some(MANUFACTURER);
    config.product = Some(PRODUCT);
    config.serial_number = Some(serial);
    config.max_power = 100;
    config.max_packet_size_0 = 64;
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 196]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 128]> = StaticCell::new();
    let mut builder = Builder::new(
        driver,
        config,
        &mut CONFIG_DESC.init([0; 256])[..],
        &mut BOS_DESC.init([0; 256])[..],
        &mut MSOS_DESC.init([0; 196])[..],
        &mut CONTROL_BUF.init([0; 128])[..],
    );

    builder.msos_descriptor(windows_version::WIN8_1, 0);

    // DAP - Custom Class 0
    static DAP_STATE: ConstStaticCell<CmsisDapV2State> =
        ConstStaticCell::new(CmsisDapV2State::new());
    let dap_class = CmsisDapV2Class::new(&mut builder, DAP_STATE.take(), 64, true);

    // CDC - dummy class to get things working for now. Windows needs more than one interface
    // to load usbccgp.sys, which is necessary for nusb to be able to list interfaces.
    static CDC_STATE: ConstStaticCell<State> = ConstStaticCell::new(State::new());
    _ = CdcAcmClass::new(&mut builder, CDC_STATE.take(), 64);

    // Build the builder.
    let usb_device = builder.build();

    // Create a dummy LED controller.
    static LED_SIGNALS: LedSignals = LedSignals {};

    // Now create the CMSIS-DAP handler.
    static SCAN_CHAIN: ConstStaticCell<[TapConfig; MAX_SCAN_CHAIN_LENGTH]> =
        ConstStaticCell::new([TapConfig::INIT; MAX_SCAN_CHAIN_LENGTH]);
    let deps = BitbangAdapter::new(
        IoPin::new(t_nrst, None::<Peri<'static, PIN_0>>),
        IoPin::new(t_jtdi, None::<Peri<'static, PIN_0>>),
        IoPin::new(t_jtms_swdio, Some(dir_swdio)),
        IoPin::new(t_jtck_swclk, Some(dir_swclk)),
        IoPin::new(t_jtdo, None::<Peri<'static, PIN_0>>),
        BitDelay,
        SCAN_CHAIN.take(),
    );

    let dap = Dap::new(
        deps,
        &LED_SIGNALS,
        BitDelay,
        None::<NoSwo>,
        concat!("2.1.0, Adaptor version ", env!("CARGO_PKG_VERSION")),
    );

    spawner.spawn(usb_task(usb_device).unwrap());
    spawner.spawn(dap_task(dap, dap_class).unwrap());
}

#[embassy_executor::task]
pub async fn usb_task(mut usb: UsbDevice<'static, Driver<'static, USB>>) {
    usb.run().await;
}

#[embassy_executor::task]
async fn dap_task(
    mut dap: Dap<
        'static,
        BitbangAdapter<IoPin<'static>, BitDelay>,
        &'static LedSignals,
        BitDelay,
        bitbang_dap::JtagAdapter<IoPin<'static>, BitDelay>,
        bitbang_dap::SwdAdapter<IoPin<'static>, BitDelay>,
        NoSwo,
    >,
    mut dap_class: CmsisDapV2Class<'static, Driver<'static, USB>>,
) {
    let mut req = [0u8; 1024];
    let mut resp = [0u8; 1024];
    loop {
        dap_class.wait_connection().await;

        let Ok(req_len) = dap_class.read_packet(&mut req).await.inspect_err(|e| {
            warn!("failed to read from USB: {:?}", e);
        }) else {
            continue;
        };

        let resp_len = dap.process_command(&req[..req_len], &mut resp, DapVersion::V2);

        if let Err(e) = dap_class.write_packet(&resp[..resp_len]).await {
            warn!("failed to write to USB: {:?}", e);
            continue;
        }
    }
}
struct BitDelay;

impl DelayNs for BitDelay {
    fn delay_ns(&mut self, ns: u32) {
        self.delay_cycles((ns as u64 * self.cpu_clock() as u64 / 1_000_000_000_u64) as u32);
    }
}

impl DelayCycles for BitDelay {
    fn delay_cycles(&mut self, cycles: u32) {
        cortex_m::asm::delay(cycles);
    }

    fn cpu_clock(&self) -> u32 {
        // This function is used to calculate the number of cycles to wait in a SWD/JTAG clock
        // cycle, so we don't actually have to return the real CPU frequency.
        // cortex_m __delay divides by 2 and Cortex-M0+ needs 4 CPU cycles per delay loop iteration.
        150_000_000 / 2
    }
}

struct IoPin<'a> {
    pin: Flex<'a>,
    direction_pin: Option<Output<'a>>,
}

impl<'a> IoPin<'a> {
    fn new(pin: Peri<'a, impl Pin>, direction_pin: Option<Peri<'a, impl Pin>>) -> Self {
        Self {
            pin: Flex::new(pin),
            direction_pin: direction_pin.map(|pin| Output::new(pin, Level::Low)),
        }
    }
}

impl InputOutputPin for IoPin<'_> {
    fn set_as_output(&mut self) {
        self.pin.set_as_output();
        if let Some(dir) = self.direction_pin.as_mut() {
            dir.set_high();
        }
    }

    fn set_high(&mut self, high: bool) {
        match high {
            true => self.pin.set_high(),
            false => self.pin.set_low(),
        }
    }

    fn set_as_input(&mut self) {
        if let Some(dir) = self.direction_pin.as_mut() {
            dir.set_low();
        }
        self.pin.set_as_input();
    }

    fn is_high(&mut self) -> bool {
        self.pin.is_high()
    }
}

struct LedSignals {}

impl DapLeds for &'static LedSignals {
    fn react_to_host_status(&mut self, _host_status: dap::HostStatus) {}
}

struct NoSwo;

impl Swo for NoSwo {
    fn set_transport(&mut self, _transport: dap_rs::swo::SwoTransport) {
        todo!()
    }

    fn set_mode(&mut self, _mode: dap_rs::swo::SwoMode) {
        todo!()
    }

    fn set_baudrate(&mut self, _baudrate: u32) -> u32 {
        todo!()
    }

    fn set_control(&mut self, _control: dap_rs::swo::SwoControl) {
        todo!()
    }

    fn polling_data(&mut self, _buf: &mut [u8]) -> u32 {
        todo!()
    }

    fn streaming_data(&mut self) {
        todo!()
    }

    fn is_active(&self) -> bool {
        todo!()
    }

    fn bytes_available(&self) -> u32 {
        todo!()
    }

    fn buffer_size(&self) -> u32 {
        todo!()
    }

    fn support(&self) -> dap_rs::swo::SwoSupport {
        todo!()
    }

    fn status(&mut self) -> dap_rs::swo::SwoStatus {
        todo!()
    }
}
