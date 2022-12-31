use question::Answer;

use crate::{state, util, ARGS};

#[derive(Debug)]
pub struct TVOptions {
    pub title: String,
    pub season: usize,
    pub episode: usize,
}

impl TVOptions {
    pub fn from_cli() -> Option<Self> {
        let enabled = ARGS.tv_mode || util::confirm("TV Show Mode", Some(Answer::NO));
        if !enabled {
            return None;
        }

        let previous_state = state::read_state();
        let mut still_using_previous = false;

        let (previous_title, previous_season, _) = previous_state.transpose();

        let title = {
            let mut title = String::new();
            if let Some(previous_title) = previous_title {
                let is_blank = previous_title.is_empty();
                let use_old_value = (!is_blank)
                    && util::confirm(&format!("Use previous title? ({})", previous_title), None);

                still_using_previous = use_old_value;
                if use_old_value {
                    title = previous_title;
                }
            }

            if title.is_empty() {
                title = loop {
                    let response = util::prompt("Please enter the title of the TV show:");
                    if !response.is_empty() {
                        break response;
                    }
                }
            }
            title
        };

        let season = {
            let mut season = None;

            if let Some(previous_season) = previous_season {
                let use_old_value = still_using_previous
                    && util::confirm(&format!("Use previous season? ({})", previous_season), None);

                if use_old_value {
                    season = Some(previous_season);
                }
            }

            if season.is_none() {
                season = loop {
                    match util::prompt("Enter the season index of the TV show:").parse::<usize>() {
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
            if let Ok(x) = util::prompt("Enter the index of the first episode in this directory:")
                .parse::<usize>()
            {
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

trait TransposeTvOptions {
    fn transpose(self) -> (Option<String>, Option<usize>, Option<usize>);
}

impl TransposeTvOptions for Option<TVOptions> {
    fn transpose(self) -> (Option<String>, Option<usize>, Option<usize>) {
        match self {
            Some(TVOptions {
                title,
                season,
                episode,
            }) => (Some(title), Some(season), Some(episode)),
            None => (None, None, None),
        }
    }
}
