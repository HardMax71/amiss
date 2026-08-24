use amiss_wire::json::Value;

/// A typed site-build observation fixture.
#[derive(Clone, Copy)]
pub enum SiteObservation<'a> {
    Page(&'a str, &'a [&'a str]),
    Redirect(&'a str),
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
        SiteObservation::Redirect(destination) => {
            members.push(("kind".to_owned(), Value::string("site-redirect".to_owned())));
            members.push((
                "destination".to_owned(),
                Value::string(destination.to_owned()),
            ));
        }
    }
    Value::object(members)
}
