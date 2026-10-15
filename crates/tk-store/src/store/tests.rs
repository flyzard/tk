//! The store's tests: text, bytes and real files, where types can't reach.

use super::*;
use proptest::prelude::*;
use serde_json::json;
use tk_core::{
    Actor, Body, Frontmatter, Kind, Label, Labels, Move, Name, NonEmpty, Priority, Resolution,
    Status, Suffix, Timestamp, Title,
};

fn key() -> ProjectKey {
    ProjectKey::try_new(String::from("app")).unwrap()
}

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Utf8Path::from_path(dir.path()).unwrap().to_owned();
    let store = Store::init(&repo, key(), String::from("App")).unwrap();
    (dir, store)
}

/// Any ticket, built from its parts through deserialization (JSON, not TOML),
/// so it is independent of the file format under test.
#[expect(
    clippy::too_many_arguments,
    clippy::needless_pass_by_value,
    reason = "one owned argument per ticket field, like a constructor"
)]
fn ticket(
    id: TicketId,
    title: Title,
    kind: Kind,
    priority: Priority,
    labels: Labels,
    depends_on: Vec<TicketId>,
    created: Timestamp,
    status: Status,
    body: Body,
) -> Ticket {
    let front: Frontmatter = serde_json::from_value(json!({
        "title": title, "kind": kind, "priority": priority, "labels": labels,
        "depends_on": depends_on, "created": created, "status": status,
    }))
    .unwrap();
    Ticket::from_stored(id, front, body)
}

prop_compose! {
    fn id()(bytes in any::<[u8; 4]>()) -> TicketId {
        TicketId::new(key(), Suffix::from_entropy(bytes))
    }
}

prop_compose! {
    fn timestamp()(seconds in 0i64..4_102_444_800, nanos in prop_oneof![Just(0i32), 0i32..1_000_000_000]) -> Timestamp {
        Timestamp::new(seconds, nanos).unwrap()
    }
}

fn title() -> impl Strategy<Value = Title> {
    "[^\n\r]{1,120}".prop_filter_map("a valid title", |s| Title::try_new(s).ok())
}

fn non_empty() -> impl Strategy<Value = NonEmpty> {
    "[^\n\r]{1,60}".prop_filter_map("non-empty", |s| NonEmpty::try_new(s).ok())
}

fn actor() -> impl Strategy<Value = Actor> {
    (any::<bool>(), "[a-z0-9._-]{1,32}").prop_map(|(human, name)| {
        let name = Name::try_new(name).unwrap();
        if human {
            Actor::Human(name)
        } else {
            Actor::Agent(name)
        }
    })
}

fn status() -> impl Strategy<Value = Status> {
    let resolution = prop_oneof![
        Just(Resolution::Completed),
        Just(Resolution::WontDo),
        id().prop_map(Resolution::Duplicate)
    ];
    prop_oneof![
        Just(Status::Todo),
        (actor(), timestamp()).prop_map(|(by, since)| Status::Doing { by, since }),
        (non_empty(), timestamp()).prop_map(|(reason, since)| Status::Blocked { reason, since }),
        (timestamp(), resolution).prop_map(|(at, resolution)| Status::Done { at, resolution }),
    ]
}

/// Any valid ticket with the given ID.
fn any_ticket(ticket_id: TicketId) -> impl Strategy<Value = Ticket> {
    let kind = prop_oneof![
        Just(Kind::Bug),
        Just(Kind::Feature),
        Just(Kind::Chore),
        Just(Kind::Spike)
    ];
    let priority = prop_oneof![
        Just(Priority::P0),
        Just(Priority::P1),
        Just(Priority::P2),
        Just(Priority::P3)
    ];
    let label = "[a-z0-9]{1,8}(-[a-z0-9]{1,8}){0,2}".prop_map(|s| Label::try_new(s).unwrap());
    (
        title(),
        kind,
        priority,
        prop::collection::btree_set(label, 0..4),
        prop::collection::vec(id(), 0..4),
        timestamp(),
        status(),
        "(?s).{0,200}".prop_map(Body::new),
    )
        .prop_map(
            move |(title, kind, priority, labels, deps, created, status, body)| {
                ticket(
                    ticket_id.clone(),
                    title,
                    kind,
                    priority,
                    labels,
                    deps,
                    created,
                    status,
                    body,
                )
            },
        )
}

proptest! {
    /// TOML and Markdown are text: render then parse gives back the same ticket.
    #[test]
    fn render_then_parse_is_identity(t in id().prop_flat_map(any_ticket)) {
        let text = format::render(&t);
        let (parsed, _) = format::parse(t.id().clone(), &text).unwrap();
        prop_assert_eq!(parsed, t);
    }

    /// The lossless path and the full render agree: saving `after` over a file
    /// rendered from `before` gives the bytes of rendering `after` from scratch.
    #[test]
    fn update_then_render_equals_fresh_render(
        (before, after) in id().prop_flat_map(|id| (any_ticket(id.clone()), any_ticket(id)))
    ) {
        let (_dir, store) = store();
        let path = store.path(before.id());
        store.transaction(|tx| -> Result<(), StoreError> {
            let _saved = tx.create(before)?;
            let id = after.id().clone();
            let edited = tx.load(&id)?.update(|_| Ok::<_, StoreError>(after.clone())).unwrap();
            let _saved = tx.save(edited)?;
            Ok(())
        }).unwrap();
        prop_assert_eq!(fs_err::read_to_string(&path).unwrap(), format::render(&after));
    }
}

const HAND_WRITTEN: &str = "+++
# Found while closing October.
title   = \"Export misses credit notes\"   # keep it short
kind = \"bug\"
priority = \"p1\"
labels = [ \"export\" ]
depends_on = []
created = 2026-10-14T09:12:00Z

status = { state = \"todo\" }
+++

## Context
Credit notes created after the 1st are missing.
";

fn hand_written(store: &Store) -> TicketId {
    let id: TicketId = "app-k3x9".parse().unwrap();
    fs_err::write(store.path(&id), HAND_WRITTEN).unwrap();
    id
}

/// Lossless editing is a property of bytes: changing one field leaves every other byte identical.
#[test]
fn changing_one_field_leaves_every_other_byte_identical() {
    let (_dir, store) = store();
    let id = hand_written(&store);
    store
        .transaction(|tx| -> Result<(), StoreError> {
            let edited = tx
                .load(&id)?
                .update(|mut t| {
                    t.priority = Priority::P0;
                    Ok::<_, StoreError>(t)
                })
                .unwrap();
            let _saved = tx.save(edited)?;
            Ok(())
        })
        .unwrap();
    let text = fs_err::read_to_string(store.path(&id)).unwrap();
    assert_eq!(
        text,
        HAND_WRITTEN.replace("priority = \"p1\"", "priority = \"p0\"")
    );
}

/// Needs real files: an edit based on a version someone else replaced is refused.
#[test]
fn saving_over_another_writers_change_is_a_conflict() {
    let (_dir, store) = store();
    let id = hand_written(&store);
    let now = Timestamp::UNIX_EPOCH;
    let block = |t: Ticket| {
        t.transition(
            Move::Block {
                reason: NonEmpty::try_new(String::from("waiting")).unwrap(),
            },
            now,
        )
    };
    let finish = |t: Ticket| {
        t.transition(
            Move::Finish {
                resolution: Resolution::Completed,
            },
            now,
        )
    };
    let result = store.transaction(|tx| -> Result<(), StoreError> {
        let mine = tx.load(&id)?;
        let theirs = tx.load(&id)?.update(block).unwrap();
        let _saved = tx.save(theirs)?;
        let _saved = tx.save(mine.update(finish).unwrap())?;
        Ok(())
    });
    assert!(
        matches!(result, Err(StoreError::Conflict(Conflict { id: conflicted, .. })) if conflicted == id)
    );
    let text = fs_err::read_to_string(store.path(&id)).unwrap();
    assert!(text.contains("state = \"blocked\""), "{text}");
}

/// Text with CRLF line endings, as a checkout without `eol=lf` produces, parses to the same ticket.
#[test]
fn crlf_files_parse() {
    let id: TicketId = "app-k3x9".parse().unwrap();
    let (lf, _) = format::parse(id.clone(), HAND_WRITTEN).unwrap();
    let (crlf, _) = format::parse(id, &HAND_WRITTEN.replace('\n', "\r\n")).unwrap();
    assert_eq!(crlf.title, lf.title);
    assert_eq!(crlf.status(), lf.status());
}
