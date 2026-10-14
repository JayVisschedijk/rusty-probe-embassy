#![no_std]
#![no_main]

use rusty_probe::{BitDelay, IoPin, LedSignals, NoSwo, dap_task, usb_task};

use bitbang_dap::BitbangAdapter;
use dap_rs::dap::Dap;
use dap_rs::jtag::TapConfig;

use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts,
    usb::{self, Driver},
    peripherals::USB,
    };
use embassy_usb::{
    self, Builder,
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
    let t_nrst = p.PIN_3;
    // let t_jtdi ;// No JTAG
    let t_jtms_swdio = p.PIN_5;
    let t_jtck_swclk = p.PIN_4;
    // let t_jtdo // No JTAG
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
        IoPin::new(t_nrst),
        IoPin::new(t_jtdi),
        IoPin::new(t_jtms_swdio),
        IoPin::new(t_jtck_swclk),
        IoPin::new(t_jtdo),
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
