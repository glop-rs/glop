#![cfg(test)]

extern crate env_logger;

use super::*;
use super::super::grammar;
use self::value::{Obj, Value};

const SIMPLE_INIT: &'static str = r#"when (message init) { }"#;
const TWO_MSGS: &'static str = r#"when (message foo, message bar) { }"#;
const SIMPLE_EQUAL: &'static str = r#"when (foo == bar) { unset foo; }"#;
const SIMPLE_NOT_EQUAL: &'static str = r#"when (foo != bar) { set foo bar; }"#;
const SIMPLE_IS_SET: &'static str = r#"when (is_set foo) { unset foo; }"#;

fn setup() {
    let _ = env_logger::init();
}

fn parse_one_match(s: &str) -> ast::Match {
    let mut g = grammar::glop(s).unwrap();
    assert_eq!(g.matches.len(), 1);
    g.matches.pop().unwrap()
}

#[test]
fn unmatched_init_empty_state() {
    setup();
    let m_ast = parse_one_match(SIMPLE_INIT);
    let mut st = State::new(MemStorage::new());
    let m_exc = Match::new_from_ast(&m_ast);
    match st.eval(m_exc).unwrap() {
        Some(_) => panic!("unexpected match"),
        None => (),
    }
}

#[test]
fn matched_init_message() {
    setup();
    let m_ast = parse_one_match(SIMPLE_INIT);
    let mut st = State::new(MemStorage::new());
    st.mut_storage().push_msg("init", Obj::new()).unwrap();
    let m_exc = Match::new_from_ast(&m_ast);
    let txn = match st.eval(m_exc.clone()).unwrap() {
        Some(mut txn) => {
            assert_eq!(txn.seq, 0);
            txn.with_context(|ctx| {
                assert!(ctx.msgs.contains_key("init"));
                assert_eq!(ctx.msgs.len(), 1);
            });
            txn
        }
        None => panic!("expected match"),
    };
    assert!(st.commit(txn).is_ok());

    match st.eval(m_exc.clone()).unwrap() {
        Some(_) => panic!("unexpected match"),
        None => {}
    }
}

#[test]
fn matched_only_init_message() {
    setup();
    let m_ast = parse_one_match(SIMPLE_INIT);
    let mut st = State::new(MemStorage::new());
    st.mut_storage().push_msg("init", Obj::new()).unwrap();
    st.mut_storage()
        .push_msg("blah",
                  [("foo".to_string(), Value::Str("bar".to_string()))]
                      .iter()
                      .cloned()
                      .collect())
        .unwrap();
    let m_exc = Match::new_from_ast(&m_ast);
    let txn = match st.eval(m_exc.clone()).unwrap() {
        Some(mut txn) => {
            assert_eq!(txn.seq, 0);
            txn.with_context(|ref mut ctx| {
                assert!(ctx.msgs.contains_key("init"));
                assert_eq!(ctx.msgs.len(), 1);
            });
            txn
        }
        None => panic!("expected match"),
    };
    assert!(st.commit(txn).is_ok());

    match st.eval(m_exc.clone()).unwrap() {
        Some(_) => panic!("unexpected match"),
        None => {}
    }
}

#[test]
fn matched_two_messages() {
    setup();
    let m_ast = parse_one_match(TWO_MSGS);
    let mut st = State::new(MemStorage::new());
    st.mut_storage().push_msg("foo", Obj::new()).unwrap();
    st.mut_storage().push_msg("bar", Obj::new()).unwrap();
    st.mut_storage().push_msg("foo", Obj::new()).unwrap();
    st.mut_storage().push_msg("bar", Obj::new()).unwrap();
    let m_exc = Match::new_from_ast(&m_ast);

    for i in 0..2 {
        let txn = match st.eval(m_exc.clone()).unwrap() {
            Some(mut txn) => {
                assert_eq!(txn.seq, i);
                txn.with_context(|ref mut ctx| {
                    assert!(ctx.msgs.contains_key("foo"));
                    assert!(ctx.msgs.contains_key("bar"));
                    assert_eq!(ctx.msgs.len(), 2);
                });
                txn
            }
            None => panic!("expected match"),
        };
        assert!(st.commit(txn).is_ok());
    }
    match st.eval(m_exc.clone()).unwrap() {
        Some(_) => panic!("unexpected match"),
        None => {}
    }
}

#[test]
fn match_equal() {
    setup();
    let m_ast = parse_one_match(SIMPLE_EQUAL);
    {
        let mut st = State::new(MemStorage::new());
        st.mut_storage().mut_vars().insert("foo".to_string(), Value::from_str("bar"));
        let m_exc = Match::new_from_ast(&m_ast);
        let txn = match st.eval(m_exc.clone()).unwrap() {
            Some(txn) => {
                assert_eq!(txn.seq, 0);
                txn
            }
            None => panic!("expected match"),
        };
        assert!(st.commit(txn).is_ok());
        // foo is now unset
        match st.eval(m_exc.clone()).unwrap() {
            Some(_) => panic!("unexpected match"),
            None => {}
        }
    }
    {
        let mut st = State::new(MemStorage::new());
        st.mut_storage().mut_vars().insert("foo".to_string(), Value::from_str("blah"));
        let m_exc = Match::new_from_ast(&m_ast);
        match st.eval(m_exc.clone()).unwrap() {
            Some(_) => panic!("unexpected match"),
            None => {}
        }
    }
}

#[test]
fn match_not_equal() {
    setup();
    let m_ast = parse_one_match(SIMPLE_NOT_EQUAL);
    {
        let mut st = State::new(MemStorage::new());
        st.mut_storage().mut_vars().insert("foo".to_string(), Value::from_str("blah"));
        let m_exc = Match::new_from_ast(&m_ast);
        match st.eval(m_exc.clone()).unwrap() {
            Some(txn) => {
                assert_eq!(txn.seq, 0);
            }
            None => panic!("expected match"),
        }
    }
    {
        let mut st = State::new(MemStorage::new());
        st.mut_storage().mut_vars().insert("foo".to_string(), Value::from_str("bar"));
        let m_exc = Match::new_from_ast(&m_ast);
        match st.eval(m_exc.clone()).unwrap() {
            Some(_) => panic!("unexpected match"),
            None => {}
        }
    }
}

#[test]
fn simple_commit_progression() {
    setup();
    let m_exc_ne = Match::new_from_ast(&parse_one_match(SIMPLE_NOT_EQUAL));
    let m_exc_eq = Match::new_from_ast(&parse_one_match(SIMPLE_EQUAL));
    let mut st = State::new(MemStorage::new());
    st.mut_storage().mut_vars().insert("foo".to_string(), Value::from_str("blah"));
    // foo starts out != bar so we expect a match and apply
    let txn = match st.eval(m_exc_ne.clone()).unwrap() {
        Some(txn) => {
            assert_eq!(txn.seq, 0);
            txn
        }
        None => panic!("expected match"),
    };
    assert!(st.commit(txn).is_ok());
    // above match sets foo == bar so m_exc_ne no longer matches
    match st.eval(m_exc_ne.clone()).unwrap() {
        Some(_) => panic!("unexpected match"),
        None => {}
    }

    // now let's match on foo == bar, should match committed state now
    let txn = match st.eval(m_exc_eq.clone()).unwrap() {
        Some(txn) => {
            assert_eq!(txn.seq, 1);
            txn
        }
        None => panic!("expected match"),
    };
    assert!(st.commit(txn).is_ok());
}

#[test]
fn match_is_set() {
    setup();
    let m_ast = parse_one_match(SIMPLE_IS_SET);
    {
        let mut st = State::new(MemStorage::new());
        st.mut_storage().mut_vars().insert("foo".to_string(), Value::from_str("bar"));
        let m_exc = Match::new_from_ast(&m_ast);
        let txn = match st.eval(m_exc.clone()).unwrap() {
            Some(txn) => {
                assert_eq!(txn.seq, 0);
                txn
            }
            None => panic!("expected match"),
        };
        assert!(st.commit(txn).is_ok());

        match st.eval(m_exc.clone()).unwrap() {
            Some(_) => panic!("unexpected match"),
            None => {}
        }
    }
    {
        let mut st = State::new(MemStorage::new());
        st.mut_storage().mut_vars().insert("bar".to_string(), Value::from_str("foo"));
        let m_exc = Match::new_from_ast(&m_ast);
        match st.eval(m_exc.clone()).unwrap() {
            Some(_) => panic!("unexpected match"),
            None => {}
        }
    }
}