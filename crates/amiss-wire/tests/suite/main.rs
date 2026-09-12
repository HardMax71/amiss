mod action;
mod control_producers;
mod external;
mod extraction;
mod human;
mod locale;
mod manifest;
mod model;
mod paths;
mod publication;
mod record;
mod relation;
#[path = "../support/relation.rs"]
mod relation_fixture;
mod report;
mod requests;
mod resolution;
mod semantic;
mod uri;
mod wire;

#[path = "../controls/main.rs"]
mod controls;

#[path = "../json_contracts/main.rs"]
mod json_contracts;

#[path = "../report_model/main.rs"]
mod report_model;
