#![no_std]

use bitbang_dap::{BitbangAdapter, DelayCycles, InputOutputPin};
use dap_rs::{
    dap::{self, Dap, DapLeds, DapVersion, DelayNs},
    swo::Swo,
};
use defmt::{todo, warn};
use embassy_rp::{
    gpio::Flex,
    peripherals::USB,
    usb::Driver,
};
use embassy_usb::{self, UsbDevice, class::cmsis_dap_v2::CmsisDapV2Class};

#[embassy_executor::task]
pub async fn usb_task(mut usb: UsbDevice<'static, Driver<'static, USB>>) {
    usb.run().await;
}

#[embassy_executor::task]
pub async fn dap_task(
    mut dap: Dap<
        'static,
        BitbangAdapter<OptIoPin<'static>, BitDelay>,
        &'static LedSignals,
        BitDelay,
        bitbang_dap::JtagAdapter<OptIoPin<'static>, BitDelay>,
        bitbang_dap::SwdAdapter<OptIoPin<'static>, BitDelay>,
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
pub struct BitDelay;

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

#[derive(Default)]
pub struct OptIoPin<'a>(Option<Flex<'a>>);

impl<'a> OptIoPin<'a> {
    pub fn new(pin: Flex<'static>) -> Self {
        Self(Some(pin))
    }
}

impl InputOutputPin for OptIoPin<'_> {
    fn set_as_output(&mut self) {
        if let OptIoPin(Some(io_pin)) = self {
            io_pin.set_as_output();
        }
    }

    fn set_high(&mut self, high: bool) {
        if let OptIoPin(Some(io_pin)) = self {
            match high {
                true => io_pin.set_high(),
                false => io_pin.set_low(),
            }
        }
    }

    fn set_as_input(&mut self) {
        if let OptIoPin(Some(io_pin)) = self {
            io_pin.set_as_input();
        }
    }

    fn is_high(&mut self) -> bool {
        if let OptIoPin(Some(io_pin)) = self {
            io_pin.is_high()
        }
        else {
            false
        }
    }
}

pub struct LedSignals {}

impl DapLeds for &'static LedSignals {
    fn react_to_host_status(&mut self, _host_status: dap::HostStatus) {}
}

pub struct NoSwo;

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
