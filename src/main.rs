use colored::Colorize;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;
use threadpool::ThreadPool;

macro_rules! parse_arg {
    ($args:expr, $short:expr, $long:expr, $var:expr, $msg:expr) => {
        if let Some(index) = $args.iter().position(|arg| arg == $short || arg == $long) {
            if let Ok(parsed_num) = $args[index + 1].parse() {
                $var = parsed_num;
            } else {
                eprintln!($msg);
                std::process::exit(1);
            }
        }
    };
}

fn is_port_open(ip: IpAddr, port: u16, timeout: u64) -> bool {
    let socket_addr = SocketAddr::new(ip, port);
    TcpStream::connect_timeout(&socket_addr, Duration::from_millis(timeout)).is_ok()
}

fn grab_banner(ip: IpAddr, port: u16, timeout: u64) -> String {
    let socket_addr = SocketAddr::new(ip, port);

    if let Ok(mut stream) = TcpStream::connect_timeout(&socket_addr, Duration::from_millis(timeout))
    {
        let mut response = Vec::new();
        let _ = stream.set_read_timeout(Some(Duration::from_millis(timeout)));
        let _ = stream.write_all(b"hai\r\n");
        let _ = stream.read_to_end(&mut response);

        return String::from_utf8_lossy(&response).to_string();
    }

    String::default()
}

fn dns_resolve(hostname: &str) -> IpAddr {
    let socket_addrs = (hostname, 0).to_socket_addrs();

    match socket_addrs {
        Ok(mut addrs) => addrs.next().unwrap().ip(),
        Err(_) => {
            eprintln!("Failed to resolve hostname");
            std::process::exit(1);
        }
    }
}

fn set_terminal_title(title: &str) {
    #[cfg(target_os = "windows")]
    {
        use winapi::um::wincon::SetConsoleTitleW;
        use winapi::um::winnt::WCHAR;

        let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let _result = unsafe { SetConsoleTitleW(wide_title.as_ptr() as *const WCHAR) };
    }

    #[cfg(not(target_os = "windows"))]
    {
        print!("\x1B]2;{}\x07", title);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!(
            "Usage: {} <IP address / hostname>\n
            \rOptions: 
            \r[-min, --minport]
            \r[-max, --maxport]
            \r[-t, --threads]
            \r[-T, --timeout]
            \r[-n, --nobanner]\n
            \rIt is not required to have both min and max ports
            \rExample: '-min 1024 -max 1337' or just '-min 22'\n
            \rDefaults: All ports, 200 threads, 1000 ms timeout\n",
            args[0]
        );
        std::process::exit(0);
    }

    let target = dns_resolve(&args[1]);
    let mut num_threads = 200;
    let mut start_port = 1;
    let mut end_port = 65535;
    let mut timeout = 1000;
    let mut get_banner = true;

    parse_arg!(&args, "-min", "--minport", start_port, "-min <port>");
    parse_arg!(&args, "-max", "--maxport", end_port, "-max <port>");
    parse_arg!(&args, "-t", "--threads", num_threads, "-t <threads>");
    parse_arg!(&args, "-T", "--timeout", timeout, "-T <timeout in ms>");

    if num_threads < 1 {
        eprintln!("Minimum 1 thread");
        std::process::exit(1);
    }

    for arg in &args {
        if arg.contains("-n") || arg.contains("--nobanner") {
            get_banner = false;
        }
    }

    let pool = ThreadPool::new(num_threads);

    println!("Scanning host: {}\n", target);

    for port in start_port..=end_port {
        pool.execute(move || {
            set_terminal_title(&format!("Probing port {}", port));

            if !is_port_open(target, port, timeout) {
                return;
            }

            if !get_banner {
                println!("Port {} is open\n", port.to_string().bright_green());
            } else {
                let banner = grab_banner(target, port, timeout);
                if !banner.is_empty() {
                    println!(
                        "Port {} is open, banner:\n\n\r{}\n",
                        port.to_string().bright_green(),
                        banner.trim_end()
                    );
                } else {
                    println!("Port {} is open\n", port.to_string().bright_green());
                }
            }
        });
    }

    pool.join();
    std::process::exit(0);
}
