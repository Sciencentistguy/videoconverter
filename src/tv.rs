use std::collections::HashMap;

use itertools::Itertools;
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
    pub fn from_cli(
        db: &Db,
        title: Option<&str>,
        filename_information: &HashMap<usize, (u32, u32)>,
    ) -> Option<Self> {
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

        let detected_season = (|| {
            // my kingdom for a try blocks
            let mut it = filename_information.iter().map(|(_, (x, _))| x).unique();
            let season = it.next()?;
            it.next().is_none().then_some(season)
        })();

        let detected_first_episode = {
            let mut episodes = filename_information
                .iter()
                .map(|(_, (_, x))| x)
                .collect::<Vec<_>>();
            episodes.sort_unstable();
            episodes
                .iter()
                .tuple_windows()
                .all(|(a, b)| a.abs_diff(**b) == 1)
                .then_some(episodes[0])
        };

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

        let season = 'a: {
            if let Some(&detected_season) = detected_season
                && util::confirm(&format!("Use detected season? ({})", detected_season), None)
            {
                break 'a detected_season;
            }
            if let Some(previous_season) = previous_season {
                let use_old_value = using_db
                    && util::confirm(&format!("Use previous season? ({})", previous_season), None);

                if use_old_value {
                    break 'a previous_season;
                } else {
                    using_db = false;
                }
            }

            loop {
                match util::prompt("Enter the season index of the TV show:").parse::<u32>() {
                    Ok(x) => break 'a x,
                    Err(_) => {
                        println!("Invalid response. Please try again.");
                    }
                }
            }
        };

        let episode = 'b: {
            if let Some(detected_first_episode) = detected_first_episode
                && util::confirm(
                    &format!("Use detected first episode? ({})", detected_first_episode),
                    None,
                )
            {
                break 'b *detected_first_episode;
            };
            loop {
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
                    break 'b x;
                }
            }
        };

        Some(TVOptions {
            title,
            season,
            episode,
        })
    }
}
