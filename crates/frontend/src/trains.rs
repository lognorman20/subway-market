#[derive(Debug, Clone)]
pub struct TrainView {
    pub dedupe_key: String,
    pub phase: String,
    pub route_id: String,
    pub trip_id: String,
    pub train_label: String,
    pub remaining_stops: Option<i32>,
    pub remaining_label: String,
    pub diff_label: String,
    pub updated_at: String,
    pub target_stop_id: String,
    pub start_stop_id: String,
    pub journey_label: String,
    pub expected_eta_label: String,
    pub frozen_at_label: String,
    pub arrival_label: String,
    pub first_seen_label: String,
    pub actual_source: String,
    pub diff_id: String,
    pub freeze_event_id: String,
    pub diff_event_id: String,
    pub action: String,
    pub side: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct TrainsPresentation {
    pub upcoming: Vec<TrainView>,
    pub history: Vec<TrainView>,
    pub banner: Option<(BannerKind, String)>,
}

impl TrainsPresentation {
    pub fn from_fetch(fetch: crate::backend::TrainsFetch) -> Self {
        match fetch {
            crate::backend::TrainsFetch::Ok(trains) => {
                let (upcoming, history) = split_upcoming_history(trains);
                let banner = if upcoming.is_empty() && history.is_empty() {
                    Some((
                        BannerKind::Info,
                        "No upcoming or history rows to display.".into(),
                    ))
                } else {
                    None
                };
                Self {
                    upcoming,
                    history,
                    banner,
                }
            }
            crate::backend::TrainsFetch::Empty => Self {
                upcoming: Vec::new(),
                history: Vec::new(),
                banner: Some((
                    BannerKind::Info,
                    "No tracked trains yet. Backend is up but the list is empty.".into(),
                )),
            },
            crate::backend::TrainsFetch::Unreachable(err) => Self {
                upcoming: Vec::new(),
                history: Vec::new(),
                banner: Some((
                    BannerKind::Warn,
                    format!("Backend unreachable ({err}). Showing empty list."),
                )),
            },
            crate::backend::TrainsFetch::HttpError(status) => Self {
                upcoming: Vec::new(),
                history: Vec::new(),
                banner: Some((
                    BannerKind::Warn,
                    format!("Backend returned HTTP {status}. Showing empty list."),
                )),
            },
            crate::backend::TrainsFetch::ParseError(err) => Self {
                upcoming: Vec::new(),
                history: Vec::new(),
                banner: Some((
                    BannerKind::Error,
                    format!("Could not parse /trains response ({err})."),
                )),
            },
        }
    }
}

/// Upcoming = watching/frozen with remaining_stops >= 1 (fewest left first).
/// History = arrived/acted or scored diff (newest first).
fn split_upcoming_history(trains: Vec<TrainView>) -> (Vec<TrainView>, Vec<TrainView>) {
    let mut upcoming = Vec::new();
    let mut history = Vec::new();

    for train in trains {
        let phase = train.phase.as_str();
        let is_history = matches!(phase, "arrived" | "acted") || train.diff_label != "—";
        let is_upcoming = matches!(phase, "watching" | "frozen")
            && train.remaining_stops.is_some_and(|n| n >= 1);

        if is_history {
            history.push(train);
        } else if is_upcoming {
            upcoming.push(train);
        }
        // drop watching/frozen with 0/null remaining and no score
    }

    upcoming.sort_by(|a, b| {
        a.remaining_stops
            .cmp(&b.remaining_stops)
            .then_with(|| a.route_id.cmp(&b.route_id))
            .then_with(|| a.trip_id.cmp(&b.trip_id))
    });
    history.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.route_id.cmp(&b.route_id))
    });

    (upcoming, history)
}
