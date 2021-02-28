use std::fs::File;
use std::io::stdin;
use std::io::stdout;
use std::io::BufRead;
use std::io::Write;

use crate::interface::TVOptions;

pub fn prompt(prompt: &str) -> std::io::Result<String> {
    let mut buf = String::new();
    print!("{}: ", prompt);

    stdout().lock().flush()?;
    stdin().read_line(&mut buf)?;
    buf.truncate(buf.trim_end().len());
    Ok(buf)
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

pub fn write_state(tv_options: &TVOptions) -> std::io::Result<()> {
    let mut file = File::create("/tmp/videoconverter.state")?;
    write!(
        &mut file,
        "{}\n{}\n{}",
        tv_options.title.as_ref().unwrap(),
        tv_options.season.as_ref().unwrap(),
        tv_options.episode.as_ref().unwrap()
    )
}

pub fn read_state() -> Result<TVOptions, Box<dyn std::error::Error>> {
    let file = File::open("/tmp/videoconverter.state")?;
    let reader = std::io::BufReader::new(file);
    let mut lines = reader
        .lines()
        .collect::<Result<Vec<String>, std::io::Error>>()?;

    if lines.len() != 3 {
        return Err("Invalid Data".into());
    }

    Ok(TVOptions {
        enabled: true,
        title: Some(std::mem::take(&mut lines[0])),
        season: Some(lines[1].parse::<usize>()?),
        episode: Some(lines[2].parse::<usize>()?),
    })
}
