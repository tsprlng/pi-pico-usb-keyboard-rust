//! Implements USB devices and tasks for transporting HID [KeyboardReport]s and CDC [crate::steno::Packet]s.
//! Mostly lifted from [embassy_usb] examples.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::UPDATES_CHANNEL;

use embassy_futures::join::join;
use embassy_rp::{
    peripherals::USB,
    usb::{Driver, InterruptHandler},
    bind_interrupts,
};
use embassy_usb::{
    class::hid::{HidReaderWriter, ReportId, RequestHandler, State as HidState},
    class::cdc_acm::{CdcAcmClass, State as CdcState},
    control::OutResponse,
    Builder, Handler, UsbDevice,
};
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use static_cell::StaticCell;

type MyDriver = Driver<'static, USB>;
type MyUsbDevice = UsbDevice<'static, MyDriver>;
type MyHidReaderWriter = HidReaderWriter<'static, MyDriver, 1, 8>;
type MyCdcAcmClass = CdcAcmClass<'static, MyDriver>;

bind_interrupts!(pub(crate) struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

pub fn get_device(driver: MyDriver) -> (UsbDevice<'static, MyDriver>, MyHidReaderWriter, MyCdcAcmClass) {
    let mut config = embassy_usb::Config::new(0xfeed, 0x3061);
    config.manufacturer = Some("Tom's");
    config.product = Some("Mini Orthocurvular Keyboard");
    config.serial_number = Some("001");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    static DEVICE_HANDLER: StaticCell<MyDeviceHandler> = StaticCell::new();

    // Create embassy-usb DeviceBuilder using the driver and config.
    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 128]> = StaticCell::new();
    let mut builder = Builder::new(
        driver,
        config,
        &mut CONFIG_DESC.init([0; 256])[..],
        &mut BOS_DESC.init([0; 256])[..],
        &mut [], // no msos descriptors
        &mut CONTROL_BUF.init([0; 128])[..],
    );

    static STATE: StaticCell<HidState> = StaticCell::new();

    builder.handler(DEVICE_HANDLER.init(MyDeviceHandler::new()));

    // Create classes on the builder.
    let config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: None,
        poll_ms: 60,
        max_packet_size: 64,
    };
    let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, STATE.init(HidState::new()), config);

    let cdc = {
        static STATE: StaticCell<CdcState> = StaticCell::new();
        let state = STATE.init(CdcState::new());
        CdcAcmClass::new(&mut builder, state, 64)
    };

    (builder.build(), hid, cdc)
}

#[embassy_executor::task]
pub async fn run(mut usb: MyUsbDevice, hid: MyHidReaderWriter, mut cdc: MyCdcAcmClass)
{
    // Run the USB device.
    let usb_fut = usb.run();

    let (reader, mut writer) = hid.split();

    // Do stuff with the class!
    let in_fut = async {
        let mut last_report: KeyboardReport = KeyboardReport::default();
        loop {
            let (report, mut steno_packet) = UPDATES_CHANNEL.receive().await;
            if report != last_report {
                match writer.write_serialize(&report).await {
                    Ok(()) => {}
                    Err(_e) => {} //warn!("Failed to send report: {:?}", e),
                };

                last_report = report;
            }
            if steno_packet.iter().any(|x| x != &0u8) && cdc.dtr() {
                // TODO possibly handle RTS pauses / disconnections better(?)
                steno_packet[0] |= 128;  // indicates lead byte of packet
                cdc.write_packet(&steno_packet).await.expect("cdc write");

                steno_packet = Default::default();
                steno_packet[0] |= 128;
                cdc.write_packet(&steno_packet).await.expect("cdc write");
            }
        }
    };

    let out_fut = async {
        static REQUEST_HANDLER: StaticCell<MyRequestHandler> = StaticCell::new();
        reader.run(false, REQUEST_HANDLER.init(MyRequestHandler {})).await;
    };

    // Run everything concurrently.
    // If we had made everything `'static` above instead, we could do this using separate tasks instead.
    join(usb_fut, join(in_fut, out_fut)).await;
}

struct MyRequestHandler;

impl RequestHandler for MyRequestHandler {
    fn get_report(&mut self, _id: ReportId, _buf: &mut [u8]) -> Option<usize> {
        //info!("Get report for {:?}", id);
        None
    }

    fn set_report(&mut self, _id: ReportId, _data: &[u8]) -> OutResponse {
        //info!("Set report for {:?}: {=[u8]}", id, data);
        OutResponse::Accepted
    }

    fn set_idle_ms(&mut self, _id: Option<ReportId>, _dur: u32) {
        //info!("Set idle rate for {:?} to {:?}", id, dur);
    }

    fn get_idle_ms(&mut self, _id: Option<ReportId>) -> Option<u32> {
        //info!("Get idle rate for {:?}", id);
        None
    }
}

struct MyDeviceHandler {
    configured: AtomicBool,
}

impl MyDeviceHandler {
    fn new() -> Self {
        MyDeviceHandler {
            configured: AtomicBool::new(false),
        }
    }
}

impl Handler for MyDeviceHandler {
    fn enabled(&mut self, enabled: bool) {
        self.configured.store(false, Ordering::Relaxed);
        if enabled {
            //info!("Device enabled");
        } else {
            //info!("Device disabled");
        }
    }

    fn reset(&mut self) {
        self.configured.store(false, Ordering::Relaxed);
        //info!("Bus reset, the Vbus current limit is 100mA");
    }

    fn addressed(&mut self, _addr: u8) {
        self.configured.store(false, Ordering::Relaxed);
        //info!("USB address set to: {}", addr);
    }

    fn configured(&mut self, configured: bool) {
        self.configured.store(configured, Ordering::Relaxed);
        if configured {
            //info!("Device configured, it may now draw up to the configured current limit from Vbus.")
        } else {
            //info!("Device is no longer configured, the Vbus current limit is 100mA.");
        }
    }
}
