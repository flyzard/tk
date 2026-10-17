//! JSON Schema for the text types, matching the rules their parsers enforce,
//! so `tk schema` tells agents what is valid before they try.

use alloc::borrow::Cow;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

use crate::entry::RefTarget;
use crate::fields::Actor;
use crate::ids::{ProjectKey, TicketId};
use crate::text::{Body, Label, Name, NextSteps, NonEmpty, Title};

macro_rules! string_schema {
    ($($ty:ident => $schema:tt),* $(,)?) => {$(
        impl JsonSchema for $ty {
            fn schema_name() -> Cow<'static, str> {
                Cow::Borrowed(stringify!($ty))
            }
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!($schema)
            }
        }
    )*};
}

string_schema! {
    ProjectKey => { "type": "string", "pattern": "^[a-z]{2,8}$" },
    TicketId => { "type": "string", "pattern": "^[a-z]{2,8}-[0-9a-hjkmnp-tv-z]{4}$", "examples": ["app-k3x9"] },
    Title => { "type": "string", "minLength": 1, "maxLength": 120, "pattern": "^[^\\n\\r]*$" },
    Label => { "type": "string", "maxLength": 32, "pattern": "^[a-z0-9]+(-[a-z0-9]+)*$" },
    Name => { "type": "string", "minLength": 1, "maxLength": 32, "pattern": "^[a-z0-9._-]+$" },
    Actor => { "type": "string", "pattern": "^(human|agent):[a-z0-9._-]{1,32}$", "examples": ["agent:claude"] },
    NonEmpty => { "type": "string", "minLength": 1, "pattern": "^[^\\n\\r]*$" },
    Body => { "type": "string", "description": "Markdown" },
    RefTarget => { "type": "string", "minLength": 1, "pattern": "^[^\\n\\r]*$",
        "description": "A ticket ID, an http(s) link, or a path from the repository root with an optional #anchor: text on the line to start at, or a Markdown heading for its section",
        "examples": ["plan.md#Handoffs between sessions", "crates/tk-app/src/brief.rs#fn build", "app-k3x9"] },
    NextSteps => { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1, "pattern": "^[^\\n\\r]*$" } },
}
