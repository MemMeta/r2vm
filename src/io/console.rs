use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, TryRecvError};
use parking_lot::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    /// Stores the tty config before the program is launched, so we can store it properly.
    static ref OLD_TTY: Mutex<Option<libc::termios>> = {
        unsafe { 
            libc::atexit(console_exit);
        }
        Mutex::new(None)
    };
}

// Regardless the destructor of Console is executed or not, we always want the tty to be restored
// when exiting. Therefore, use atexit to guard this.
extern "C" fn console_exit() {
    let mut guard = OLD_TTY.lock();
    if let Some(ref tty) = guard.take() {
        unsafe { libc::tcsetattr(0, libc::TCSANOW, tty) };
    }
}

pub struct Console {
    rx: Mutex<Receiver<u8>>,
}

impl Drop for Console {
    fn drop(&mut self) {
        console_exit();
    }
}

impl Console {
    pub fn new() -> Console {
        let mut guard = OLD_TTY.lock();
        // It's an error to create a new console while previous one isn't cleaned up.
        if guard.is_some() { panic!("Console can only be initialized once") }

        // Make tty as raw terminal
        unsafe {
            let mut tty: libc::termios = std::mem::uninitialized();
            libc::tcgetattr(0, &mut tty);
            *guard = Some(tty);
            libc::cfmakeraw(&mut tty);
            // Still treat \n as \r\n, for convience of logging
            tty.c_oflag |= libc::OPOST;
            tty.c_cc[libc::VMIN] = 1;
            tty.c_cc[libc::VTIME] = 0;
            libc::tcsetattr(0, libc::TCSANOW, &tty);
        }

        // Spawn a thread to handle keyboard inputs.
        // In the future this thread may also use epolls etc to handle other IOs.
        // We spawn a new thread instead of using non-blocking and let guest OS to pull us so we can
        // terminate the process using Ctrl+A X whenever we like.
        let (tx, rx) = std::sync::mpsc::channel::<u8>();
        std::thread::Builder::new().name("console".to_owned()).spawn(move || {
            let mut buffer = 0;
            loop {
                // Just read a single character
                std::io::stdin().read_exact(std::slice::from_mut(&mut buffer)).unwrap();

                // Ctrl + A hit, read another and do corresponding action
                if buffer == 1 {
                    std::io::stdin().read_exact(std::slice::from_mut(&mut buffer)).unwrap();
                    match buffer {
                        b't' => {
                            crate::shutdown(crate::ExitReason::SetThreaded(!crate::threaded()));
                            continue
                        }
                        b'x' => {
                            println!("Terminated");
                            crate::shutdown(crate::ExitReason::Exit(0));
                        }
                        b'c' => {
                            unsafe { libc::raise(libc::SIGTRAP); }
                        }
                        // Hit Ctrl + A twice, send Ctrl + A to guest
                        1 => (),
                        // Ignore all other characters
                        _ => continue,
                    }
                }
                tx.send(buffer).unwrap();
            }
        }).unwrap();

        Console {
            rx: Mutex::new(rx),
        }
    }

    pub fn send(&self, data: &[u8]) -> std::io::Result<usize> {
        let mut out = std::io::stdout();
        out.write_all(data)?;
        out.flush()?;
        Ok(data.len())
    }

    pub fn try_recv(&self, data: &mut [u8]) -> std::io::Result<usize> {
        let rx = match CONSOLE.rx.try_lock() {
            Some(v) => v,
            None => return Ok(0),
        };
        let mut len = 0;
        while len < data.len() {
            match rx.try_recv() {
                Ok(key) => {
                    data[len] = key;
                    len += 1;
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => unreachable!(),
            }
        }
        Ok(len)
    }

    pub fn recv(&self, data: &mut [u8]) -> std::io::Result<usize> {
        if data.len() == 0 { return Ok(0) }
        match CONSOLE.rx.lock().recv() {
            Ok(key) => {
                data[0] = key;
            },
            Err(_) => unreachable!(),
        }
        Ok(self.try_recv(&mut data[1..])? + 1)
    }
}

lazy_static! {
    pub static ref CONSOLE: Console = {
        Console::new()
    };
}

pub fn console_init() {
    lazy_static::initialize(&CONSOLE);
}

pub fn console_putchar(char: u8) {
    CONSOLE.send(std::slice::from_ref(&char)).unwrap();
}

pub fn console_getchar() -> i64 {
    let mut ret = 0;
    match CONSOLE.try_recv(std::slice::from_mut(&mut ret)).unwrap() {
        0 => -1,
        _ => ret as i64,
    }
}
