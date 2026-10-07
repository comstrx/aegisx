use std::collections::{BTreeMap, HashMap};

use foldhash::fast::RandomState;

use crate::core::matcher::Matcher;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pattern;

pub struct Entry <T> {
    pub(super) value  : T,
    pub(super) exact  : bool,
    pub(super) prefer : bool,
}

pub enum Scope {
    Any,
    Host(String),
    Suffix(String),
}

pub type Rule <T> = ( Scope, regex::Regex, T );

pub struct Router <T> {
    pub(super) exact    : HashMap<String, Matcher<Vec<Entry<T>>>, RandomState>,
    pub(super) wildcard : Vec<( String, Matcher<Vec<Entry<T>>> )>,
    pub(super) any      : Matcher<Vec<Entry<T>>>,
    pub(super) single   : Option<T>,
    pub(super) rules    : Vec<Rule<T>>,
}

pub struct Draft <T> {
    pub(super) exact    : HashMap<String, BTreeMap<String, Vec<Entry<T>>>>,
    pub(super) wildcard : BTreeMap<String, BTreeMap<String, Vec<Entry<T>>>>,
    pub(super) any      : BTreeMap<String, Vec<Entry<T>>>,
    pub(super) rules    : Vec<Rule<T>>,
}
