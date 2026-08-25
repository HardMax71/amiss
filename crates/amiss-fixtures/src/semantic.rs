use amiss_wire::json::Value;

/// A typed site-build observation fixture.
#[derive(Clone, Copy)]
pub enum SiteObservation<'a> {
    Page(&'a str, &'a [&'a str]),
    Generated(Option<&'a str>, &'a [&'a str]),
    Redirect(&'a str, &'a str),
}

/// Builds one site-build semantic observation.
#[must_use]
pub fn site_observation(route: &str, observation: SiteObservation<'_>) -> Value {
    let mut members = vec![("route".to_owned(), Value::string(route.to_owned()))];
    match observation {
        SiteObservation::Page(source, anchors) => {
            members.push(("kind".to_owned(), Value::string("site-route".to_owned())));
            members.push(("source".to_owned(), Value::string(source.to_owned())));
            members.push((
                "anchors".to_owned(),
                Value::array(
                    anchors
                        .iter()
                        .map(|anchor| Value::string((*anchor).to_owned()))
                        .collect(),
                ),
            ));
        }
        SiteObservation::Generated(source, anchors) => {
            members.push((
                "kind".to_owned(),
                Value::string("site-generated-route".to_owned()),
            ));
            members.push((
                "source".to_owned(),
                source.map_or(Value::Null, |source| Value::string(source.to_owned())),
            ));
            members.push((
                "anchors".to_owned(),
                Value::array(
                    anchors
                        .iter()
                        .map(|anchor| Value::string((*anchor).to_owned()))
                        .collect(),
                ),
            ));
        }
        SiteObservation::Redirect(source, destination) => {
            members.push(("kind".to_owned(), Value::string("site-redirect".to_owned())));
            members.push(("source".to_owned(), Value::string(source.to_owned())));
            members.push((
                "destination".to_owned(),
                Value::string(destination.to_owned()),
            ));
        }
    }
    Value::object(members)
}

/// Builds one canonical site navigation observation.
#[must_use]
pub fn site_navigation(
    root: Option<&str>,
    manifest: &str,
    entrypoints: &[&str],
    reachable: &[&str],
) -> Value {
    Value::object(vec![
        (
            "entrypoints".to_owned(),
            Value::array(
                entrypoints
                    .iter()
                    .map(|route| Value::string((*route).to_owned()))
                    .collect(),
            ),
        ),
        (
            "kind".to_owned(),
            Value::string("site-navigation".to_owned()),
        ),
        ("manifest".to_owned(), Value::string(manifest.to_owned())),
        (
            "reachable".to_owned(),
            Value::array(
                reachable
                    .iter()
                    .map(|source| Value::string((*source).to_owned()))
                    .collect(),
            ),
        ),
        (
            "root".to_owned(),
            root.map_or(Value::Null, |root| Value::string(root.to_owned())),
        ),
    ])
}
