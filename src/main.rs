use colored::Colorize;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Duration, sleep, timeout},
};

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

struct Rand {
    state: u8,
}

impl Rand {
    fn new() -> Self {
        let seed: u8 = rand::random();
        Self { state: seed }
    }

    fn next(&mut self) -> u8 {
        self.state ^= self.state << 7;
        self.state ^= self.state >> 5;
        self.state ^= self.state << 3;
        self.state
    }
}

async fn is_port_open(ip: IpAddr, port: u16, timeout_ms: Duration) -> bool {
    let socket_addr = SocketAddr::new(ip, port);
    match timeout(timeout_ms, TcpStream::connect(&socket_addr)).await {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

async fn grab_banner(ip: IpAddr, port: u16, timeout_ms: Duration) -> String {
    let addr = SocketAddr::new(ip, port);
    let mut stream = match timeout(timeout_ms, TcpStream::connect(addr)).await {
        Ok(Ok(s)) => s,
        _ => return String::new(),
    };

    let mut rng = Rand::new();
    let mut data: Vec<u8> = Vec::with_capacity(5);
    for _ in 0..data.capacity() {
        data.push(rng.next() / 2 /*128 max*/);
    }

    let string: String = data.iter().map(|&x| x as char).collect();
    let probe = format!("{}\r\n", string);
    let mut response = Vec::new();

    if let Err(_) = stream.write_all(probe.as_bytes()).await {
        return String::new();
    }

    match timeout(timeout_ms, stream.read_to_end(&mut response)).await {
        Ok(Ok(_)) => String::from_utf8_lossy(&response).to_string(),
        _ => String::new(),
    }
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

#[tokio::main]
async fn main() {
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
    let mut timeout_ms = 1000;
    let mut get_banner = true;

    parse_arg!(&args, "-min", "--minport", start_port, "-min <port>");
    parse_arg!(&args, "-max", "--maxport", end_port, "-max <port>");
    parse_arg!(&args, "-t", "--threads", num_threads, "-t <threads>");
    parse_arg!(&args, "-T", "--timeout", timeout_ms, "-T <timeout in ms>");

    if num_threads < 1 {
        eprintln!("Minimum 1 thread");
        std::process::exit(1);
    }

    for arg in &args {
        if arg.contains("-n") || arg.contains("--nobanner") {
            get_banner = false;
        }
    }

    // limit the number of async tasks
    let sem = std::sync::Arc::new(Semaphore::new(num_threads));

    println!("Scanning host: {}\n", target);
    for port in start_port..=end_port {
        let permit = sem.clone().acquire_owned().await.unwrap();
        tokio::spawn(async move {
            set_terminal_title(&format!("Probing port {}", port));

            // force a 50 ms sleep so it's not too fast on LAN (low latency)
            let start = Instant::now();
            if !is_port_open(target, port, Duration::from_millis(timeout_ms)).await {
                sleep(Duration::from_millis(50)).await;
                drop(permit);
                return;
            }

            let now = Instant::now();
            if now.duration_since(start) < Duration::from_millis(50) {
                sleep(Duration::from_millis(50) - now.duration_since(start)).await
            }

            if !get_banner {
                println!("Port {} is open\n", port.to_string().bright_green());
            } else {
                let banner = grab_banner(target, port, Duration::from_millis(timeout_ms)).await;
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
            drop(permit);
        });
    }
}
