use std::collections::HashMap;

use super::ast;

#[derive(Debug)]
pub enum Value {
    Int(i32),
    Str(String),
    Object(Obj),
}

impl Clone for Value {
    fn clone(&self) -> Value {
        match self {
            &Value::Int(i) => Value::Int(i),
            &Value::Str(ref s) => Value::Str(s.clone()),
            &Value::Object(ref o) => Value::Object(o.clone()),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        match self {
            &Value::Int(x) => {
                match other {
                    &Value::Int(y) => x == y,
                    _ => false,
                }
            }
            &Value::Str(ref x) => {
                match other {
                    &Value::Str(ref y) => x == y,
                    _ => false,
                }
            }
            &Value::Object(ref x) => {
                match other {
                    &Value::Object(ref y) => x.eq(y),
                    _ => false,
                }
            }
        }
    }
}

impl Value {
    pub fn from_int(i: i32) -> Value {
        Value::Int(i)
    }
    pub fn from_str(s: &str) -> Value {
        Value::Str(s.to_string())
    }
    pub fn from_obj(o: Obj) -> Value {
        Value::Object(o)
    }
}

pub type Obj = HashMap<String, Value>;

pub struct Identifier(Vec<String>);

impl Identifier {
    pub fn from_ast(i_ast: &ast::Identifier) -> Identifier {
        Identifier(i_ast.clone())
    }

    pub fn from_str(s: &str) -> Identifier {
        let v: Vec<String> = s.split(".").map(|x| x.to_string()).collect();
        Identifier(v)
    }

    pub fn get<'b>(&self, root: &'b Obj) -> Option<&'b Value> {
        if self.0.is_empty() {
            return None;
        }
        let mut cur = root;
        for i in 0..self.0.len() {
            match cur.get(&self.0[i]) {
                Some(v) => {
                    match v {
                        &Value::Object(ref o) => {
                            cur = o;
                        }
                        _ => {}
                    }
                    if i == self.0.len() - 1 {
                        return Some(v);
                    }
                }
                None => {
                    return None;
                }
            }
        }
        return None;
    }

    pub fn is_set(&self, root: &Obj) -> bool {
        match self.get(root) {
            Some(_) => true,
            None => false,
        }
    }

    pub fn set(&self, root: &mut Obj, value: Value) {
        Identifier::set_slice(&self.0, root, value);
    }

    fn set_slice(path: &[String], o: &mut Obj, value: Value) {
        if path.is_empty() {
            return;
        }
        let (next, rest) = path.split_first().unwrap();
        if rest.is_empty() {
            o.insert(next.to_string(), value);
            return;
        }
        match o.get_mut(next) {
            Some(v) => {
                match v {
                    &mut Value::Object(ref mut child) => {
                        Identifier::set_slice(rest, child, value);
                        return;
                    }
                    _ => {}
                }
            }
            None => {}
        }
        o.insert(next.to_string(), Value::Object(Obj::new()));
        Identifier::set_slice(path, o, value);
    }

    pub fn unset(&self, root: &mut Obj) {
        Identifier::unset_slice(&self.0, root)
    }

    fn unset_slice(path: &[String], o: &mut Obj) {
        if path.is_empty() {
            return;
        }
        let (next, rest) = path.split_first().unwrap();
        if rest.is_empty() {
            o.remove(next);
            return;
        }
        match o.get_mut(next) {
            Some(v) => {
                match v {
                    &mut Value::Object(ref mut child) => {
                        Identifier::unset_slice(rest, child);
                        return;
                    }
                    _ => {}
                }
            }
            None => {}
        }
    }
}
