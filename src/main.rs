use {
    itertools::Itertools,
    log::warn,
    reqwest::{
        header::{HeaderValue, COOKIE},
        Client,
    },
    std::{error::Error, ffi::OsStr, fs::read_to_string, num::ParseIntError},
    structopt::StructOpt,
    unhtml::{
        Error as UnhtmlError,
        scraper::{Html, Node, Selector},
        ElemIter,
        FromHtml,
    },
    unhtml_derive::FromHtml,
};

const PROJECT_EULER_HOSTNAME: &str = "projecteuler.net";
const PROGRESS_ENDPOINT: &str = "progress";
const SESSION_COOKIE_NAME: &str = "PHPSESSID";

fn default_session_id_path() -> &'static OsStr {
    SESSION_COOKIE_NAME.as_ref()
}

#[derive(Debug, StructOpt)]
#[structopt(about, author)]
struct Cli {
    session_id: Option<String>,
}

#[derive(Debug)]
struct Level {
    description: String,
    completed: bool,
}

#[derive(Debug)]
struct Levels(Vec<Level>);

#[derive(Debug)]
enum LevelLinkParseError<'a> {
    SplitFailed(&'a str),
    ParseFailed(ParseIntError),
}

fn parse_from_relative_link<'h>(
    thing: &str,
    href: &'h str,
) -> Result<usize, LevelLinkParseError<'h>> {
    use self::LevelLinkParseError::*;

    match href.split('=').collect_tuple() {
        Some((thing, level)) => Ok(level.parse().map_err(ParseFailed)?),
        _ => Err(SplitFailed(href)),
    }
}

impl FromHtml for Levels {
    fn from_elements(iter: ElemIter) -> Result<Self, UnhtmlError> {
        let mut levels = Vec::new();

        let selector = Selector::parse("div.info a").unwrap();
        for anchor_el in iter {
            use self::Node::*;

            let level =
                parse_from_relative_link("level", anchor_el.value().attr("href").unwrap()).unwrap();
            let expected_idx = levels.len().checked_add(1).unwrap();
            if level != expected_idx {
                panic!("Missing expected level {}", expected_idx);
            }

            match anchor_el
                .children()
                .collect_tuple()
                .map(|(rt, ds)| (rt.value(), ds))
            {
                Some((Element(resolution_tag), description_span)) => levels.push(Level {
                    description: match description_span
                        .children()
                        .map(|nr| nr.value())
                        .collect_tuple()
                    {
                        Some((Element(title), Text(description)))
                            if &*title.name.local == "div" =>
                        {
                            format!("{}", description.text)
                        }
                        _ => panic!(
                            "unexpected description format in level {}: {:#?}",
                            level, description_span
                        ),
                    },
                    completed: match &*resolution_tag.name.local {
                        "div" => false,
                        "img" => true,
                        _ => panic!(
                            "unrecognized completion tag in level {}: {:#?}",
                            level, resolution_tag
                        ),
                    },
                }),
                _ => panic!(
                    "unrecognized format underneath anchor in level {}: {:#?}",
                    level, anchor_el
                ),
            }
        }

        Ok(Levels(levels))
    }
}

#[derive(Debug)]
struct Problems(Vec<bool>);

impl FromHtml for Problems {
    fn from_elements(iter: ElemIter) -> Result<Self, UnhtmlError> {
        use self::Node::*;

        let mut problems = Vec::new();

        let selector = Selector::parse("td.problem_solved,td.problem_unsolved").unwrap();
        for problem_el in iter {
            let mut solved = None;
            for class in problem_el.value().classes.iter() {
                let class: &str = &*class;
                let solved_value = match class {
                    "problem_solved" => true,
                    "problem_unsolved" => false,
                    _ => {
                        warn!(
                            "unable to determine solution status from class \"{}\"",
                            class
                        );
                        continue;
                    }
                };
                assert!(solved.is_none());
                solved = Some(solved_value);
            }
            let solved = solved.expect("unable to find solution status");
            match problem_el.children().map(|nr| nr.value()).collect_tuple() {
                Some((Element(anchor),)) if &*anchor.name.local == "a" => {
                    let link = anchor.attr("href").unwrap();
                    let level = parse_from_relative_link("problem", link).unwrap();
                    let expected_idx = problems.len().checked_add(1).unwrap();
                    if level != expected_idx {
                        panic!("Missing expected problem {}", expected_idx);
                    }
                    problems.push(solved);
                }
                _ => panic!(
                    "unrecognized set of child elements in problem listing: {:#?}",
                    problem_el.value()
                ),
            }
        }

        Ok(Problems(problems))
    }
}

#[derive(Debug, FromHtml)]
struct Progress {
    #[html(selector = "#levels_completed_section")]
    levels: Levels,
    #[html(selector = "#problems_solved_section")]
    problems: Problems,
}

fn main() -> Result<(), Box<dyn Error>> {
    let Cli { session_id } = Cli::from_args();

    let request_url = format!("https://{}/{}", PROJECT_EULER_HOSTNAME, PROGRESS_ENDPOINT);
    let session_cookie_value = match session_id {
        Some(value) => value,
        None => read_to_string(default_session_id_path())?,
    };
    let cookie_header = format!("{}={}", SESSION_COOKIE_NAME, session_cookie_value.trim());

    let mut progress_response = Client::new()
        .get(&request_url)
        .header(COOKIE, HeaderValue::from_str(&cookie_header)?)
        .send()?;
    let progress_page = progress_response.text()?;
    let progress = Progress::from_html(&progress_page)?;
    println!("progress: {:#?}", progress);
    Ok(())
}
