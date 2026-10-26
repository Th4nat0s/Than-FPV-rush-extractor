//! Browser rules, independent of window rendering.
use fpv_library::{FlightView, SessionView};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    #[default]
    Sessions,
    Flights,
    Figures,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Longest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Browse {
    pub view: View,
    pub session: Option<String>,
    pub filter_session: Option<String>,
    pub liked: bool,
    pub labels: Vec<String>,
    pub all_labels: bool,
    pub figure_labels: Vec<String>,
    pub sort: Sort,
    pub page: usize,
    pub page_size: usize,
}

impl Default for Browse {
    fn default() -> Self {
        Self {
            view: View::Sessions,
            session: None,
            filter_session: None,
            liked: false,
            labels: vec![],
            all_labels: false,
            figure_labels: vec![],
            sort: Sort::Newest,
            page: 0,
            page_size: 24,
        }
    }
}

pub fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

impl Browse {
    pub fn matches(&self, session: &SessionView, flight: &FlightView) -> bool {
        let wanted = if self.view == View::Sessions {
            &self.session
        } else {
            &self.filter_session
        };
        if wanted.as_ref().is_some_and(|id| id != &session.id) || (self.liked && !flight.liked) {
            return false;
        }
        if self.view == View::Figures || self.labels.is_empty() {
            return true;
        }
        let has = |label: &String| {
            flight
                .labels
                .iter()
                .chain(&session.labels)
                .any(|tag| same(tag, label))
        };
        if self.all_labels {
            self.labels.iter().all(has)
        } else {
            self.labels.iter().any(has)
        }
    }

    pub fn flights<'a>(
        &self,
        sessions: &'a [SessionView],
    ) -> Vec<(&'a SessionView, &'a FlightView)> {
        let mut items: Vec<_> = sessions
            .iter()
            .flat_map(|session| session.flights.iter().map(move |flight| (session, flight)))
            .filter(|(s, f)| self.matches(s, f))
            .collect();
        items.sort_by(|(a, af), (b, bf)| match self.sort {
            Sort::Longest => bf.duration.total_cmp(&af.duration),
            sort => {
                let order = a
                    .date
                    .cmp(&b.date)
                    .then_with(|| a.id.cmp(&b.id))
                    .then_with(|| af.order.cmp(&bf.order));
                if sort == Sort::Newest {
                    order.reverse()
                } else {
                    order
                }
            }
        });
        items
    }
}

pub fn page_range(total: usize, page: &mut usize, size: usize) -> std::ops::Range<usize> {
    let size = size.max(1);
    *page = (*page).min(total.saturating_sub(1) / size);
    let start = page.saturating_mul(size);
    start..start.saturating_add(size).min(total)
}

pub fn clock(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    format!("{}:{:04.1}", (seconds / 60.0) as u64, seconds % 60.0)
}

pub fn flight_name(flight: &FlightView) -> String {
    flight
        .title
        .clone()
        .unwrap_or_else(|| format!("Flight {}", flight.order))
}

pub fn labels(text: &str) -> Vec<String> {
    let mut result: Vec<String> = vec![];
    for label in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !result.iter().any(|known| same(known, label)) {
            result.push(label.into());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_deleted_pages_clamp_without_overflow() {
        let mut page = usize::MAX;
        assert_eq!(page_range(0, &mut page, 0), 0..0);
        page = 5;
        assert_eq!(page_range(25, &mut page, 24), 24..25);
        assert_eq!(page, 1);
    }
    #[test]
    fn entered_labels_preserve_spelling_and_ignore_case_duplicates() {
        assert_eq!(
            labels(" Gates,freestyle, gates,, "),
            vec!["Gates", "freestyle"]
        );
    }
    #[test]
    fn invalid_clock_input_is_safe() {
        assert_eq!(clock(f64::NAN), "0:00.0");
        assert_eq!(clock(-1.0), "0:00.0");
    }

    fn flight(id: &str, order: i64, liked: bool, labels: &[&str], duration: f64) -> FlightView {
        FlightView {
            id: id.into(),
            order,
            title: None,
            liked,
            duration,
            clips: vec![],
            recordings: 1,
            action_cam: false,
            source: "DVR".into(),
            format: String::new(),
            reduced: false,
            cache_key: id.into(),
            labels: labels.iter().map(|s| (*s).into()).collect(),
            figures: vec![],
        }
    }

    fn session(
        id: &str,
        date: Option<&str>,
        flights: Vec<FlightView>,
        labels: &[&str],
    ) -> SessionView {
        SessionView {
            id: id.into(),
            name: id.into(),
            date: date.map(str::to_owned),
            files: vec![],
            flights,
            segmentable: true,
            reducible: false,
            detection_profile: "standard".into(),
            location: None,
            notes: None,
            labels: labels.iter().map(|s| (*s).into()).collect(),
        }
    }

    #[test]
    fn filters_combine_session_and_flight_tags_without_case() {
        let sessions = vec![
            session(
                "race",
                None,
                vec![
                    flight("one", 1, true, &["freestyle", "Gates"], 60.0),
                    flight("two", 2, false, &["gates"], 20.0),
                ],
                &["Race"],
            ),
            session(
                "alps",
                None,
                vec![flight("three", 1, true, &["cinematic"], 360.0)],
                &[],
            ),
        ];
        let mut browse = Browse {
            view: View::Flights,
            ..Browse::default()
        };
        assert_eq!(browse.flights(&sessions).len(), 3);
        browse.labels = vec!["race".into(), "FREESTYLE".into()];
        browse.all_labels = true;
        assert_eq!(
            browse
                .flights(&sessions)
                .iter()
                .map(|(_, f)| f.id.as_str())
                .collect::<Vec<_>>(),
            ["one"]
        );
        browse.all_labels = false;
        assert_eq!(browse.flights(&sessions).len(), 2);
        browse.liked = true;
        assert_eq!(browse.flights(&sessions).len(), 1);
        browse.filter_session = Some("alps".into());
        assert!(browse.flights(&sessions).is_empty());
    }

    #[test]
    fn sorts_undated_sessions_then_dates_then_flight_order() {
        let sessions = vec![
            session(
                "alps",
                Some("2026-10-17"),
                vec![flight("long", 1, false, &[], 360.0)],
                &[],
            ),
            session(
                "race",
                Some("2026-08-23"),
                vec![
                    flight("one", 1, false, &[], 60.0),
                    flight("two", 2, false, &[], 20.0),
                ],
                &[],
            ),
            session(
                "unknown",
                None,
                vec![flight("old", 1, false, &[], 10.0)],
                &[],
            ),
        ];
        let mut browse = Browse {
            view: View::Flights,
            sort: Sort::Oldest,
            ..Browse::default()
        };
        let ids = |browse: &Browse| {
            browse
                .flights(&sessions)
                .iter()
                .map(|(_, f)| f.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&browse), ["old", "one", "two", "long"]);
        browse.sort = Sort::Newest;
        assert_eq!(ids(&browse), ["long", "two", "one", "old"]);
        browse.sort = Sort::Longest;
        assert_eq!(ids(&browse), ["long", "one", "two", "old"]);
    }
}
