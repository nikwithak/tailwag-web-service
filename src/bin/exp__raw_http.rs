use std::{io::Read, net::TcpListener};

pub fn main() -> Result<(), std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:8889")?;
    while let Ok((mut stream, socket)) = listener.accept() {
        let thread = std::thread::spawn(move || {
            let mut contents = String::new();
            stream.read_to_string(&mut contents).unwrap();
            println!("{}", &contents);
        });
        thread.join().unwrap();
    }
    Ok(())
}
