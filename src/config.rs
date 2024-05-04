use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::KeyRef;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Configuration<T> {
    #[serde(flatten)]
    data: HashMap<PathBuf, HashMap<String, T>>,
}

impl<T> Configuration<T> {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn keys<'a>(&'a self, path: impl AsRef<Path>) -> impl Iterator<Item = KeyRef> + 'a {
        let paths: Vec<_> = self
            .data
            .keys()
            .filter(|v| v.starts_with(path.as_ref()))
            .collect();
        paths
            .into_iter()
            .filter_map(|path| self.data.get(path).map(|v| (path, v)))
            .flat_map(|(path, v)| {
                v.keys().map(|k| KeyRef {
                    key: k.clone(),
                    path: path.to_path_buf(),
                })
            })
    }

    pub fn get_all(&self, path: impl AsRef<Path>) -> HashMap<&String, &T> {
        let paths: Vec<_> = path.as_ref().ancestors().collect();
        paths
            .into_iter()
            .rev()
            .filter_map(|path| {
                let res = self.data.get(path);
                res
            })
            .fold(HashMap::new(), |mut acc, values| {
                acc.extend(values.iter());
                acc
            })
    }

    pub fn set(&mut self, key: KeyRef, value: impl Into<T>) -> Option<T> {
        self.data
            .entry(key.path)
            .or_default()
            .insert(key.key, value.into())
    }

    pub fn get(&self, key_ref: &KeyRef) -> Option<&T> {
        key_ref
            .path
            .ancestors()
            .filter_map(|path| self.data.get(path).and_then(|map| map.get(&key_ref.key)))
            .next()
    }

    pub fn remove(&mut self, key: &KeyRef) -> Option<T> {
        self.data
            .get_mut(&key.path)
            .and_then(|kv| kv.remove(&key.key))
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl<T> Configuration<T>
where
    T: Default + std::fmt::Display,
{
    pub fn display(&self) -> String {
        let iter = self.data.keys().map(|key| key.iter());
        let mut tree = ArenaTree::default();
        for mut paths in iter {
            let Some(base) = paths.next() else {
                return "".to_string();
            };
            let mut current_path = PathBuf::from(base);
            let mut current = tree.node(PathData {
                path: current_path.clone(),
                data: self.data.get(&current_path),
            });
            for p in paths {
                let new_path = current_path.join(p);
                let next = tree.node(PathData {
                    path: new_path.clone(),
                    data: self.data.get(&new_path),
                });
                match tree.arena[next].parent {
                    Some(_) => {}
                    None => {
                        tree.arena[next].parent = Some(current);
                        tree.arena[current].children.push(next);
                    }
                }
                current = next;
                current_path = new_path;
            }
        }
        self.print_node(&tree, 0, "")
    }

    fn print_node(&self, tree: &ArenaTree<PathData<T>>, index: usize, prefix: &str) -> String {
        let mut res = String::new();
        let depth = tree.get_depth(index);
        let node = &tree.arena[index];
        let mut prefix = String::from(prefix);
        if depth == 0 {
            res += &format!("{}\n", node.value.path.display());
        } else {
            // if depth is greater than 0 then node has parent
            let parent = &tree.arena[node.parent.unwrap()];
            let is_last_child = parent.children.last().unwrap() == &node.idx;
            let pipe_char = "\u{2502} ";
            let tree_char = if is_last_child {
                "\u{2514}\u{2500}"
            } else {
                "\u{251C}\u{2500}"
            };
            res += &format!(
                "{}{}\n",
                tree_char,
                PathBuf::from(node.value.path.file_name().unwrap()).display(),
            );

            prefix.push_str(if is_last_child { "  " } else { pipe_char });
        }
        if let Some(data) = node.value.data {
            for (i, (key, value)) in data.iter().enumerate() {
                let tree_char = if i == data.len() - 1 && node.children.is_empty() {
                    "\u{2514}\u{2500}"
                } else {
                    "\u{251C}\u{2500}"
                };
                res += &format!("{prefix}{tree_char}{key}: {value}\n");
            }
        }
        for child_index in &node.children {
            res += &format!("{}{}", prefix, self.print_node(tree, *child_index, &prefix));
        }
        res
    }
}

#[derive(Debug, Default)]
pub struct ArenaTree<T>
where
    T: PartialEq,
{
    arena: Vec<Node<T>>,
}

#[derive(Debug)]
pub struct Node<T>
where
    T: PartialEq,
{
    idx: usize,
    value: T,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl<T: PartialEq> Node<T> {
    fn new(idx: usize, value: T) -> Self {
        Self {
            idx,
            value,
            parent: None,
            children: Vec::new(),
        }
    }
}

impl<T: PartialEq> ArenaTree<T> {
    fn node(&mut self, value: T) -> usize {
        if let Some(node) = self.arena.iter().find(|node| node.value == value) {
            return node.idx;
        }
        let index = self.arena.len();
        self.arena.push(Node::new(index, value));
        index
    }

    fn get_depth(&self, index: usize) -> usize {
        let mut depth = 0;
        let mut parent = self.arena[index].parent;
        while let Some(parent_index) = parent {
            depth += 1;
            parent = self.arena[parent_index].parent;
        }
        depth
    }
}

#[derive(Default)]
struct PathData<'a, T: Default> {
    path: PathBuf,
    data: Option<&'a HashMap<String, T>>,
}

impl<'a, T: Default> PartialEq for PathData<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

#[cfg(test)]
mod tests {

    use std::path::Path;

    use super::*;

    fn get_config(cwd: impl AsRef<Path>) -> Configuration<String> {
        Configuration {
            cwd: PathBuf::from(cwd.as_ref()),
            data: HashMap::from_iter([
                (
                    PathBuf::from("/"),
                    HashMap::from_iter([
                        ("foo".into(), "bar1".into()),
                        ("fem".into(), "is_great".into()),
                    ]),
                ),
                (
                    PathBuf::from("/foo"),
                    HashMap::from_iter([("foo".into(), "bar2".into())]),
                ),
                (
                    PathBuf::from("/foo/bar"),
                    HashMap::from_iter([("foo".into(), "bar3".into())]),
                ),
            ]),
        }
    }

    #[test]
    fn get_all_values() {
        let config = get_config("/foo/bar");
        let all_values = config.get_values_for_cwd();
        assert_eq!(
            all_values,
            HashMap::from_iter([
                (&String::from("foo"), &String::from("bar3")),
                (&String::from("fem"), &String::from("is_great"))
            ])
        );
    }

    #[test]
    fn add_value() {
        let mut config = get_config("/foo/bar");
        config.set("uri", "foo");
        let result = config.get_value("uri").unwrap();
        assert_eq!(result, "foo");
    }

    #[test]
    fn get_value() {
        let config = get_config("/foo/bar");
        assert_eq!(config.get_value("foo"), Some(&String::from("bar3")));
        assert_eq!(config.get_value("fem"), Some(&String::from("is_great")));

        let config = config.with_path("/foo");
        assert_eq!(config.get_value("foo"), Some(&String::from("bar2")));

        let config = config.with_path("/");
        assert_eq!(config.get_value("foo"), Some(&String::from("bar1")));
    }

    #[test]
    fn remove_value() {
        let mut config = get_config("/foo/bar");
        let res = config.remove_value("foo").unwrap();
        assert_eq!(res, String::from("bar3"));
        assert_eq!(config.get_value("foo"), Some(&String::from("bar2")));
    }
}
