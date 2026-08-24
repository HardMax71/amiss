use amiss_wire::json::Value;

/// Builds one site-route semantic observation.
#[must_use]
pub fn site_route(route: &str, source: &str, anchors: &[&str]) -> Value {
    Value::object(vec![
        ("kind".to_owned(), Value::string("site-route".to_owned())),
        ("route".to_owned(), Value::string(route.to_owned())),
        ("source".to_owned(), Value::string(source.to_owned())),
        (
            "anchors".to_owned(),
            Value::array(
                anchors
                    .iter()
                    .map(|anchor| Value::string((*anchor).to_owned()))
                    .collect(),
            ),
        ),
    ])
}
