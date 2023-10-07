use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
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

fn is_port_open(ip: &str, port: u16, timeout: u64) -> bool
{
    let ip_addr: Result<IpAddr, _> = ip.parse();
    if let Ok(ip_addr) = ip_addr {
        let socket_addr = SocketAddr::new(ip_addr, port);
        TcpStream::connect_timeout(&socket_addr, Duration::from_secs(timeout)).is_ok()
    } else {
        eprintln!("Could not parse ip: {}", ip);
        std::process::exit(1);
    }
}

fn grab_banner(ip: &str, port: u16, timeout: u64) -> String
{
    let ip_addr: Result<IpAddr, _> = ip.parse();
    if let Ok(ip_addr) = ip_addr {
        let socket_addr = SocketAddr::new(ip_addr, port);

        if let Ok(mut stream) =
            TcpStream::connect_timeout(&socket_addr, Duration::from_secs(timeout))
        {
            let mut response = Vec::new();
            let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout)));
            let _ = stream.write_all(b"hai\r\n");
            let _ = stream.read_to_end(&mut response);

            return String::from_utf8_lossy(&response).to_string();
        }
    }

    String::default()
}

fn dns_resolve(hostname: &str) -> String
{
    let socket_addrs = (hostname, 0).to_socket_addrs();

    if let Ok(mut addrs) = socket_addrs {
        if let Some(addr) = addrs.next() {
            return addr.ip().to_string();
        }
    } else {
        eprintln!("Hostname resolve failed");
        std::process::exit(1);
    }

    String::default()
}

fn set_terminal_title(title: &str)
{
    // i hate microsoft
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

fn print_colored_text(text: &str, color: Color)
{
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    let mut color_spec = ColorSpec::new();

    color_spec.set_fg(Some(color));
    stdout.set_color(&color_spec).unwrap();

    write!(stdout, "{}", text).unwrap();
    stdout.reset().unwrap();
}

fn main()
{
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
            \rDefaults: All ports, 200 threads, 1 second timeout\n",
            args[0]
        );
        std::process::exit(0);
    }

    let target = dns_resolve(&args[1]);
    let mut num_threads = 200;
    let mut start_port = 1;
    let mut end_port = 65535;
    let mut timeout = 1;
    let mut get_banner = true;

    parse_arg!(&args, "-min", "--minport", start_port, "-min <port>");
    parse_arg!(&args, "-max", "--maxport", end_port, "-max <port>");
    parse_arg!(&args, "-t", "--threads", num_threads, "-t <threads>");
    parse_arg!(&args, "-T", "--timeout", timeout, "-T <timeout in seconds>");

    if num_threads < 1 {
        eprintln!("Minimum 1 thread");
        std::process::exit(1);
    }

    for arg in &args {
        if arg.contains("-n") || arg.contains("--nobanner") {
            get_banner = false;
        }
    }

    let stdout = Arc::new(Mutex::new(StandardStream::stdout(ColorChoice::Always)));
    let pool = ThreadPool::new(num_threads);

    for port in start_port..=end_port {
        let target = target.to_string();
        let stdout_clone = Arc::clone(&stdout);

        pool.execute(move || {
            let stdout = stdout_clone.lock().unwrap();
            set_terminal_title(&format!("Probing {} on port {}", target, port));
            drop(stdout);

            if is_port_open(&target, port, timeout) {
                let stdout = stdout_clone.lock().unwrap();

                print!("\nPort ");
                print_colored_text(&port.to_string(), Color::Green);

                if get_banner {
                    let banner = grab_banner(&target, port, timeout);
                    if !banner.is_empty() {
                        println!(" is open, banner:\n\n\r{}", banner.trim_end());
                    } else {
                        println!(" is open");
                    }
                } else {
                    println!(" is open");
                }

                drop(stdout);
            }
        });
    }

    pool.join();
    std::process::exit(0);
}
