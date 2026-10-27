//! Loci: a small ontology over the garden, after the commonplace book's *loci communes*.
//!
//! A note can say what kind of thing it's about (`locus:`) and name, in front matter,
//! the things it relates to through a handful of typed relations. Things without a
//! note of their own become generated locus pages, the way tags do.
//!
//! Guardrails: there is no locus for people, places carry no coordinates, and every
//! relation is one the author wrote. Nothing is inferred.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};

use crate::content::{Diagnostic, FrontMatter, Garden, Level, slugify};
use crate::links::{Index, Target};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Work,
    Place,
    Artefact,
    Question,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Work, Kind::Place, Kind::Artefact, Kind::Question];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Work => "work",
            Kind::Place => "place",
            Kind::Artefact => "artefact",
            Kind::Question => "question",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            Kind::Work => "works",
            Kind::Place => "places",
            Kind::Artefact => "artefacts",
            Kind::Question => "questions",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relation {
    ReadAlongside,
    PartOf,
    Traversed,
    AttendsTo,
    Raises,
    Displaces,
}

impl Relation {
    pub const ALL: [Relation; 6] = [
        Relation::PartOf,
        Relation::ReadAlongside,
        Relation::Traversed,
        Relation::AttendsTo,
        Relation::Displaces,
        Relation::Raises,
    ];

    /// What a target becomes when it has no note of its own.
    pub fn target_kind(self) -> Kind {
        match self {
            Relation::ReadAlongside | Relation::PartOf => Kind::Work,
            Relation::Traversed => Kind::Place,
            Relation::AttendsTo | Relation::Displaces => Kind::Artefact,
            Relation::Raises => Kind::Question,
        }
    }

    /// Read from the note outwards: "Under the Greenwood Tree — part of → Wessex".
    pub fn label(self) -> &'static str {
        match self {
            Relation::ReadAlongside => "read alongside",
            Relation::PartOf => "part of",
            Relation::Traversed => "traversed",
            Relation::AttendsTo => "attends to",
            Relation::Raises => "raises",
            Relation::Displaces => "displaces",
        }
    }

    /// Read from the target back: "Wessex — includes → Under the Greenwood Tree".
    pub fn inverse(self) -> &'static str {
        match self {
            Relation::ReadAlongside => "read alongside",
            Relation::PartOf => "includes",
            Relation::Traversed => "traversed in",
            Relation::AttendsTo => "attended to in",
            Relation::Raises => "raised in",
            Relation::Displaces => "displaced in",
        }
    }
}

/// `read-alongside: One Title` and `read-alongside: [A, B]` both work.
pub fn one_or_many<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(s) => vec![s],
        OneOrMany::Many(v) => v,
    })
}

/// Front matter keys that would pin a place to a point on a map. Places stay at the
/// level of a town or region, so these are refused outright.
pub const LOCATION_FIELDS: &[&str] = &[
    "coordinates",
    "coords",
    "lat",
    "lng",
    "lon",
    "latitude",
    "longitude",
    "geo",
    "gps",
    "address",
    "location",
];

fn relations(meta: &FrontMatter) -> [(Relation, &[String]); 6] {
    [
        (Relation::PartOf, &meta.part_of),
        (Relation::ReadAlongside, &meta.read_alongside),
        (Relation::Traversed, &meta.traversed),
        (Relation::AttendsTo, &meta.attends_to),
        (Relation::Displaces, &meta.displaces),
        (Relation::Raises, &meta.raises),
    ]
}

/// Either a published note or a generated locus page.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Thing {
    Note(usize),
    Locus(Kind, String),
}

/// A thing named in front matter that has no note of its own.
#[derive(Debug)]
pub struct Locus {
    pub kind: Kind,
    pub name: String,
    pub slug: String,
    /// Notes that name it, and how.
    pub mentions: Vec<(usize, Relation)>,
}

/// "In note N, `new` displaces `old`."
#[derive(Debug)]
pub struct Displacement {
    pub note: usize,
    pub new: Thing,
    pub old: Thing,
}

#[derive(Debug)]
pub struct Edge {
    pub relation: Relation,
    pub target: Thing,
    /// For `displaces`: what the target displaces.
    pub old: Option<Thing>,
}

#[derive(Debug, Default)]
pub struct Ontology {
    pub loci: BTreeMap<(Kind, String), Locus>,
    /// Per note, the relations it declares.
    pub outgoing: Vec<Vec<Edge>>,
    /// Per note, the relations other notes declare towards it.
    pub incoming: Vec<Vec<(usize, Relation)>>,
    pub displacements: Vec<Displacement>,
}

impl Ontology {
    fn thing(
        &mut self,
        index: &Index,
        found: &mut Vec<Diagnostic>,
        garden: &Garden,
        from: usize,
        name: &str,
        relation: Relation,
    ) -> Option<Thing> {
        let path = &garden.notes[from].path;
        match index.resolve(name, from) {
            Target::Note { index: to, .. } => Some(Thing::Note(to)),
            // Front matter is metadata, not prose: drop it entirely rather than
            // print an unpublished note's name anywhere.
            Target::Private => {
                found.push(Diagnostic {
                    level: Level::Warning,
                    path: path.clone(),
                    line: None,
                    message: format!(
                        "`{}: {name}` names an unpublished note, so it's left out",
                        relation.label().replace(' ', "-")
                    ),
                });
                None
            }
            Target::Missing => {
                let slug = slugify(name);
                if slug.is_empty() {
                    return None;
                }
                let kind = relation.target_kind();
                let locus = self
                    .loci
                    .entry((kind, slug.clone()))
                    .or_insert_with(|| Locus {
                        kind,
                        name: name.trim().to_string(),
                        slug: slug.clone(),
                        mentions: Vec::new(),
                    });
                if !locus.mentions.contains(&(from, relation)) {
                    locus.mentions.push((from, relation));
                }
                Some(Thing::Locus(kind, slug))
            }
        }
    }
}

/// Build the ontology from every published note's front matter.
pub fn build(garden: &mut Garden, index: &Index) -> Ontology {
    let mut ontology = Ontology {
        outgoing: garden.notes.iter().map(|_| Vec::new()).collect(),
        incoming: garden.notes.iter().map(|_| Vec::new()).collect(),
        ..Ontology::default()
    };
    let mut found = Vec::new();

    for from in 0..garden.notes.len() {
        let declared: Vec<(Relation, Vec<String>)> = relations(&garden.notes[from].meta)
            .into_iter()
            .map(|(r, names)| (r, names.to_vec()))
            .collect();

        for (relation, names) in declared {
            for name in names {
                if relation == Relation::Displaces {
                    let Some((new, old)) = name.split_once('>').or_else(|| name.split_once('→'))
                    else {
                        found.push(Diagnostic {
                            level: Level::Warning,
                            path: garden.notes[from].path.clone(),
                            line: None,
                            message: format!(
                                "`displaces: {name}` should read `new thing > old thing`"
                            ),
                        });
                        continue;
                    };
                    let new = ontology.thing(index, &mut found, garden, from, new, relation);
                    let old = ontology.thing(index, &mut found, garden, from, old, relation);
                    if let (Some(new), Some(old)) = (new, old) {
                        ontology.displacements.push(Displacement {
                            note: from,
                            new: new.clone(),
                            old: old.clone(),
                        });
                        ontology.outgoing[from].push(Edge {
                            relation,
                            target: new,
                            old: Some(old),
                        });
                    }
                    continue;
                }

                let Some(target) = ontology.thing(index, &mut found, garden, from, &name, relation)
                else {
                    continue;
                };
                if let Thing::Note(to) = target {
                    if to == from {
                        continue;
                    }
                    if !ontology.incoming[to].contains(&(from, relation)) {
                        ontology.incoming[to].push((from, relation));
                    }
                }
                ontology.outgoing[from].push(Edge {
                    relation,
                    target,
                    old: None,
                });
            }
        }
    }

    garden.diagnostics.extend(found);
    ontology
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Note;
    use crate::links;
    use std::path::PathBuf;

    fn note(stem: &str, meta: FrontMatter) -> Note {
        Note {
            path: PathBuf::from(format!("{stem}.md")),
            slug: slugify(stem),
            title: stem.to_string(),
            meta,
            body: String::new(),
            body_line: 1,
        }
    }

    fn build_for(notes: Vec<Note>, private: &[&str]) -> (Garden, Ontology) {
        let mut garden = Garden {
            notes,
            private_names: private.iter().map(|p| p.to_string()).collect(),
            ..Garden::default()
        };
        let index = links::build(&mut garden, false);
        let ontology = build(&mut garden, &index);
        (garden, ontology)
    }

    #[test]
    fn relations_point_at_notes_or_generate_loci() {
        let greenwood = FrontMatter {
            part_of: vec!["Wessex".into()],
            traversed: vec!["Dorset".into()],
            ..FrontMatter::default()
        };
        let (_, o) = build_for(
            vec![
                note("Greenwood", greenwood),
                note("Wessex", FrontMatter::default()),
            ],
            &[],
        );
        assert_eq!(o.outgoing[0][0].target, Thing::Note(1));
        assert_eq!(o.incoming[1], vec![(0, Relation::PartOf)]);
        assert_eq!(
            o.outgoing[0][1].target,
            Thing::Locus(Kind::Place, "dorset".into())
        );
        assert_eq!(
            o.loci[&(Kind::Place, "dorset".into())].mentions,
            vec![(0, Relation::Traversed)]
        );
    }

    #[test]
    fn displaces_reads_new_over_old() {
        let meta = FrontMatter {
            displaces: vec!["The organ > the quire".into(), "no arrow here".into()],
            ..FrontMatter::default()
        };
        let (garden, o) = build_for(vec![note("A", meta)], &[]);
        let d = &o.displacements[0];
        assert_eq!(d.new, Thing::Locus(Kind::Artefact, "the-organ".into()));
        assert_eq!(d.old, Thing::Locus(Kind::Artefact, "the-quire".into()));
        assert_eq!(
            garden.diagnostics.len(),
            1,
            "malformed displaces should warn"
        );
    }

    #[test]
    fn relations_to_private_notes_are_dropped() {
        let meta = FrontMatter {
            read_alongside: vec!["Secret Diary".into()],
            ..FrontMatter::default()
        };
        let (garden, o) = build_for(vec![note("A", meta)], &["Secret Diary"]);
        assert!(o.outgoing[0].is_empty());
        assert!(o.loci.is_empty());
        assert_eq!(garden.diagnostics.len(), 1);
    }
}
