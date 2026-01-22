use question::Answer;
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::{ARGS, state::Db, util};

#[derive(Debug, Deserialize, Serialize)]
pub struct TVOptions {
    pub title: String,
    pub season: u32,
    pub episode: u32,
}

impl TVOptions {
    pub fn from_cli(db: &Db, title: Option<&str>) -> Option<Self> {
        if ARGS.tv_mode {
            trace!("TV Mode enabled via args");
            return Some(TVOptions {
                title: ARGS.title.clone().unwrap(),
                season: ARGS.season.unwrap(),
                episode: ARGS.episode.unwrap(),
            });
        }

        if !util::confirm("TV Show Mode", Some(Answer::NO)) {
            return None;
        }

        let mut using_db = false;

        let previous_state = title.and_then(|title| db.find(title));
        let guessed_title = previous_state.as_ref().map(|state| state.title.clone());

        let title = {
            if let Some(guessed_title) = guessed_title
                && util::confirm(
                    &format!("Use guessed title? ({guessed_title})"),
                    Some(Answer::YES),
                )
            {
                using_db = true;
                guessed_title
            } else {
                loop {
                    let response = util::prompt("Please enter the title of the TV show:");
                    if !response.is_empty() {
                        break response;
                    }
                }
            }
        };

        let previous_season = previous_state.as_ref().map(|state| state.season);

        let season = {
            let mut season = None;

            if let Some(previous_season) = previous_season {
                let use_old_value = using_db
                    && util::confirm(&format!("Use previous season? ({})", previous_season), None);

                if use_old_value {
                    season = Some(previous_season);
                }
            }

            if season.is_none() {
                season = loop {
                    match util::prompt("Enter the season index of the TV show:").parse::<u32>() {
                        Ok(x) => break Some(x),
                        Err(_) => {
                            println!("Invalid response. Please try again.");
                            continue;
                        }
                    }
                }
            }
            season.unwrap()
        };

        let episode = loop {
            let prompt = if let Some(previous_state) = &previous_state
                && using_db
            {
                format!(
                    "Enter the index of the first episode in this directory (previous: {}):",
                    previous_state.episode
                )
            } else {
                "Enter the index of the first episode in this directory:".to_string()
            };
            if let Ok(x) = util::prompt(&prompt).parse::<u32>() {
                break x;
            }
        };

        Some(TVOptions {
            title,
            season,
            episode,
        })
    }
}
