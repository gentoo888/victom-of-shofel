use nix::{ioctl_none, ioctl_readwrite_bad, ioctl_write_ptr};
use std::fs::{File, OpenOptions};
use std::os::unix::io::{AsRawFd, RawFd};

const RAW_GADGET_MAGIC: u8 = b'U';
const UDC_NAME_LENGTH_MAX: usize = 128;

// =========================================================================
//  KERNEL STRUCTS
// =========================================================================

#[repr(C, packed)]
pub struct UsbRawInit {
    pub driver_name: [u8; UDC_NAME_LENGTH_MAX],
    pub device_name: [u8; UDC_NAME_LENGTH_MAX],
    pub speed: u8,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct UsbCtrlRequest {
    pub b_request_type: u8,
    pub b_request: u8,
    pub w_value: u16,
    pub w_index: u16,
    pub w_length: u16,
}

#[repr(C)]
pub struct UsbRawEvent {
    pub event_type: u32,
    pub length: u32,
}

// =========================================================================
//  IOCTL DEFINITIONS
// =========================================================================

ioctl_write_ptr!(raw_gadget_init, RAW_GADGET_MAGIC, 0, UsbRawInit);
ioctl_none!(raw_gadget_run, RAW_GADGET_MAGIC, 1);

const RAW_EVENT_IOCTL_NUM: u32 = 0x80085502;
ioctl_readwrite_bad!(raw_gadget_event_fetch, RAW_EVENT_IOCTL_NUM, u8);

// =========================================================================
//  EVENT TYPES
// =========================================================================

#[derive(Debug)]
pub enum Event {
    Connect,
    Control(UsbCtrlRequest),
    Disconnect,
    Reset,
    Suspend,
    Resume,
    VbusDraw,
    Unknown(u32),
}

// =========================================================================
//  USB INITIALIZATION
// =========================================================================

pub fn init_usb() -> std::io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/raw-gadget")?;
    let fd = file.as_raw_fd();

    println!("  Opened /dev/raw-gadget. FD: {}", fd);

    let mut init_data = UsbRawInit {
        driver_name: [0; UDC_NAME_LENGTH_MAX],
        device_name: [0; UDC_NAME_LENGTH_MAX],
        speed: 3, // USB_SPEED_HIGH
    };

    let d_name = b"dummy_udc\0";
    let dev_name = b"dummy_udc.0\0";

    init_data.driver_name[..d_name.len()].copy_from_slice(d_name);
    init_data.device_name[..dev_name.len()].copy_from_slice(dev_name);

    unsafe {
        raw_gadget_init(fd, &init_data).map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
        println!("  INIT successful");

        raw_gadget_run(fd).map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
        println!("  Device is active!");
    }

    Ok(file)
}

// =========================================================================
//  EVENT FETCH
// =========================================================================

pub fn event_fetch(fd: RawFd) -> std::io::Result<Event> {
    let mut event_buffer = vec![0u8; 1024];

    unsafe {
        raw_gadget_event_fetch(fd, event_buffer.as_mut_ptr())
            .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
    }

    // First 4 bytes: event_type
    let event_type = u32::from_ne_bytes([
        event_buffer[0],
        event_buffer[1],
        event_buffer[2],
        event_buffer[3],
    ]);

    // Kernel event types:
    // 0 = CONNECT, 1 = CONTROL, 2 = SUSPEND, 3 = RESUME,
    // 4 = RESET, 5 = DISCONNECT, 6 = VBUS_DRAW
    let event = match event_type {
        0 => Event::Connect,
        1 => {
            // SETUP packet is at event_buffer[8..16]
            let setup = unsafe {
                let ptr = event_buffer[8..16].as_ptr() as *const UsbCtrlRequest;
                UsbCtrlRequest {
                    b_request_type: std::ptr::read_unaligned(std::ptr::addr_of!(
                        (*ptr).b_request_type
                    )),
                    b_request: std::ptr::read_unaligned(std::ptr::addr_of!((*ptr).b_request)),
                    w_value: std::ptr::read_unaligned(std::ptr::addr_of!((*ptr).w_value)),
                    w_index: std::ptr::read_unaligned(std::ptr::addr_of!((*ptr).w_index)),
                    w_length: std::ptr::read_unaligned(std::ptr::addr_of!((*ptr).w_length)),
                }
            };
            Event::Control(setup)
        }
        2 => Event::Suspend,
        3 => Event::Resume,
        4 => Event::Reset,
        5 => Event::Disconnect,
        6 => Event::VbusDraw,
        _ => Event::Unknown(event_type),
    };

    Ok(event)
}

// =========================================================================
//  EVENT HANDLER
// =========================================================================

pub fn handle_event(fd: RawFd, event: Event) -> std::io::Result<()> {
    match event {
        Event::Connect => {
            println!(" CONNECT: Host connected to us");
        }
        Event::Control(setup) => {
            log_control_request(&setup);
            handle_setup(fd, setup)?;
        }
        Event::Disconnect => {
            println!(" DISCONNECT");
        }
        Event::Reset => {
            println!("󰑙 RESET");
        }
        Event::Suspend => {
            println!("󰏤 SUSPEND");
        }
        Event::Resume => {
            println!(" RESUME");
        }
        Event::VbusDraw => {
            println!(" VBUS_DRAW: Host started drawing power");
        }
        Event::Unknown(t) => {
            println!(" Host sent an unknown event: {}", t);
        }
    }
    Ok(())
}

// I really need this to understand the control requests much more easier without having to start a
// 4 dimensional google research everytime
fn log_control_request(setup: &UsbCtrlRequest) {
    let b_request_type = setup.b_request_type;
    let b_request = setup.b_request;
    let w_value = setup.w_value;
    let w_index = setup.w_index;
    let w_length = setup.w_length;

    // Direction
    let direction = if b_request_type & 0x80 != 0 {
        "Device->Host"
    } else {
        "Host->Device"
    };

    // Type
    let req_type = match (b_request_type >> 5) & 0x03 {
        0 => "Standard",
        1 => "Class",
        2 => "Vendor",
        _ => "Reserved",
    };

    // Recipient
    let recipient = match b_request_type & 0x1F {
        0 => "Device",
        1 => "Interface",
        2 => "Endpoint",
        3 => "Other",
        _ => "Reserved",
    };

    // Request
    let request_name = match b_request {
        0x00 => "GET_STATUS",
        0x01 => "CLEAR_FEATURE",
        0x03 => "SET_FEATURE",
        0x05 => "SET_ADDRESS",
        0x06 => "GET_DESCRIPTOR",
        0x07 => "SET_DESCRIPTOR",
        0x08 => "GET_CONFIGURATION",
        0x09 => "SET_CONFIGURATION",
        0x0A => "GET_INTERFACE",
        0x0B => "SET_INTERFACE",
        0x0C => "SYNCH_FRAME",
        _ => "UNKNOWN",
    };

    // Descriptor type (for GET_DESCRIPTOR)
    let descriptor_type = if b_request == 0x06 {
        match (w_value >> 8) as u8 {
            1 => " (DEVICE)",
            2 => " (CONFIGURATION)",
            3 => " (STRING)",
            4 => " (INTERFACE)",
            5 => " (ENDPOINT)",
            _ => "",
        }
    } else {
        ""
    };

    println!(
        "Host sent a CONTROL request to me: {} | {} | {} | {}{} | Value=0x{:04X}, Index=0x{:04X}, Length={}",
        direction, req_type, recipient, request_name, descriptor_type, w_value, w_index, w_length
    );
}

fn handle_setup(fd: RawFd, setup: UsbCtrlRequest) -> std::io::Result<()> {
    // TODO: GET_DESCRIPTOR, GET_STATUS, SET_ADDRESS, SET_CONFIGURATION
    let _ = fd;
    let _ = setup;
    Ok(())
}
