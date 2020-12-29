use std::io::{stdin, stdout, Write};

pub fn prompt(prompt: &str) -> std::io::Result<String> {
    let mut buf = String::new();
    print!("{}: ", prompt);

    stdout().lock().flush()?;
    stdin().read_line(&mut buf)?;
    buf.make_ascii_lowercase();
    buf.truncate(buf.trim_end().len());
    return Ok(buf);
}

pub fn confirm(prompt: &str, default: bool) -> std::io::Result<bool> {
    let mut buf = String::new();
    loop {
        if default {
            print!("{} (Y/n) ", prompt);
        } else {
            print!("{} (y/N) ", prompt);
        }

        stdout().lock().flush()?;
        stdin().read_line(&mut buf)?;
        buf.make_ascii_lowercase();

        match &*(buf.trim_end()) {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            "" => return Ok(default),
            _ => println!("Invalid response."),
        }
    }
}
