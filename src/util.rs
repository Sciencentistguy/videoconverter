use color_eyre::eyre::{Result, eyre};
use question::{Answer, Question};

pub fn prompt(prompt: &str) -> Result<String> {
    loop {
        match Question::new(prompt).ask() {
            Some(Answer::RESPONSE(s)) if s.is_empty() => continue,
            Some(Answer::RESPONSE(s)) => break Ok(s),
            Some(answer) => {
                break Err(eyre!("Unexpected answer to a free-text prompt: {answer:?}"));
            }
            None => break Err(eyre!("Failed to read a response to '{prompt}'")),
        }
    }
}

pub fn confirm(prompt: &str, default: Option<question::Answer>) -> Result<bool> {
    let mut question = Question::new(prompt);
    question.yes_no().show_defaults();

    if let Some(default) = default {
        question.default(default);
    } else {
        question.until_acceptable();
    }

    match question.confirm() {
        Answer::YES => Ok(true),
        Answer::NO => Ok(false),
        Answer::RESPONSE(x) => Err(eyre!(
            "Unexpected answer to a yes/no prompt '{prompt}': `{x}`"
        )),
    }
}
