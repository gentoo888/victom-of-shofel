mod usb;

fn main() -> std::io::Result<()> {
    println!(
        "!!!NOTICE: DO NOT TURN OFF OR DISABLE RAW-GADGET ON YOUR KERNEL WHILE VICTOM IS RUNNING. IF YOU DO, YOU WILL GET A KERNEL PANIC!!!"
    );
    println!("=== THIS IS THE VICTOM OF SHOFEL ===");
    println!("I hate my life"); // Top 10 times actors weren't acting
    // USB'yi başlat
    let file = usb::init_usb()?;
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);

    // Ana döngü
    loop {
        match usb::event_fetch(fd) {
            Ok(event) => {
                usb::handle_event(fd, event)?;
            }
            Err(e) => {
                eprintln!("ERROR: I couldn't get the event: {}", e);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }
}
