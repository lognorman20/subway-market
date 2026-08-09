//! Stop id helpers for human-readable logs.

/// Cardinal direction from MTA stop id suffix (`N` / `S`).
pub fn direction_label(stop_id: &str) -> Option<&'static str> {
    match stop_id.chars().last()? {
        'N' => Some("northbound"),
        'S' => Some("southbound"),
        _ => None,
    }
}

/// Best-effort stop label for logs: `A32 (Inwood-207 St, northbound)`.
pub fn format_stop(stop_id: &str) -> String {
    let name = stop_name(stop_id);
    let dir = direction_label(stop_id);
    match (name, dir) {
        (Some(name), Some(dir)) => format!("{stop_id} ({name}, {dir})"),
        (Some(name), None) => format!("{stop_id} ({name})"),
        (None, Some(dir)) => format!("{stop_id} ({dir})"),
        (None, None) => stop_id.to_string(),
    }
}

fn stop_name(stop_id: &str) -> Option<&'static str> {
    let base = stop_id.trim_end_matches(['N', 'S']);
    match base {
        "A02" => Some("Inwood-207 St"),
        "A09" => Some("168 St"),
        "A24" => Some("34 St-Penn Station"),
        "A27" => Some("Chambers St"),
        "A32" => Some("Far Rockaway-Mott Av"),
        "A41" => Some("Howard Beach-JFK"),
        "A46" => Some("Lefferts Blvd"),
        "B04" => Some("Bedford Park Blvd"),
        "B08" => Some("145 St"),
        "B16" => Some("Brighton Beach"),
        "D01" => Some("Norwood-205 St"),
        "D13" => Some("Coney Island-Stillwell Av"),
        "G05" => Some("Court Sq"),
        "G22" => Some("Church Av"),
        "L01" => Some("8 Av"),
        "L29" => Some("Canarsie-Rockaway Pkwy"),
        "N10" => Some("Astoria-Ditmars Blvd"),
        "R01" => Some("Forest Hills-71 Av"),
        "R29" => Some("Bay Ridge-95 St"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_from_suffix() {
        assert_eq!(direction_label("A24N"), Some("northbound"));
        assert_eq!(direction_label("A24S"), Some("southbound"));
        assert_eq!(direction_label("A24"), None);
    }

    #[test]
    fn format_stop_includes_name_and_direction() {
        let label = format_stop("A32N");
        assert!(label.contains("A32N"));
        assert!(label.contains("Far Rockaway"));
        assert!(label.contains("northbound"));
    }
}
