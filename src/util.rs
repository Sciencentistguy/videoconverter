use question::{Answer, Question};

pub fn prompt(prompt: &str) -> String {
    loop {
        match Question::new(prompt).ask() {
            Some(Answer::RESPONSE(s)) if s.is_empty() => continue,
            Some(Answer::RESPONSE(s)) => break s,
            Some(_) => unreachable!("Not a yes/no question"),
            _ => unreachable!("Question::ask() should never return None"),
        }
    }
}

pub fn confirm(prompt: &str, default: Option<question::Answer>) -> bool {
    let mut question = Question::new(prompt);
    question.yes_no().show_defaults();

    if let Some(default) = default {
        question.default(default);
    } else {
        question.until_acceptable();
    }

    match question.confirm() {
        Answer::YES => true,
        Answer::NO => false,
        Answer::RESPONSE(x) => unreachable!("Yes/No Question shouldn't return RESPONSE: `{x}`"),
    }
}
