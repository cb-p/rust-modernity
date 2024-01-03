use std::{
    collections::{HashMap, VecDeque},
    fs::File,
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use syn::UseTree;

#[derive(deluxe::ExtractAttributes)]
#[deluxe(attributes(stable))]
struct Stable {
    #[allow(dead_code)]
    pub feature: String,
    pub since: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct VersionedItem {
    #[serde(skip)]
    name: String,
    version: String,
    public: bool,
    children: HashMap<String, VersionedItem>,
}

impl VersionedItem {
    pub fn new(name: String) -> VersionedItem {
        Self {
            name,
            version: "1.0.0".to_string(),
            public: true,
            children: HashMap::new(),
        }
    }

    // pub fn dump_all_to_stdout(&self, prefix: &str) {
    //     println!("{prefix} = {}", self.version);
    //     for (name, item) in self.children.iter() {
    //         item.dump_all_to_stdout(&format!("{prefix}::{name}"));
    //     }
    // }
}

// impl Default for VersionedItem {
//     fn default() -> Self {
//         Self {
//             version: "1.0.0".to_string(),
//             public: true,
//             children: HashMap::new(),
//         }
//     }
// }

#[derive(Serialize, Deserialize, Debug, Clone)]
enum LocalAlias {
    Named(String),
    GlobChildren,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Alias {
    root: Vec<String>,
    relative_path: Vec<String>,
    local: LocalAlias,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct VersionConstructor {
    root: VersionedItem,
    aliases: Vec<Alias>,

    #[serde(skip)]
    path_stack: VecDeque<String>,
}

impl VersionConstructor {
    pub fn new() -> VersionConstructor {
        VersionConstructor {
            root: VersionedItem::new("".to_string()),
            aliases: Vec::new(),
            path_stack: VecDeque::new(),
        }
    }

    pub fn process_file(&mut self, name: String, file: syn::File) {
        self.push_path(name);
        for item in file.items {
            self.process_item(item);
        }
        self.pop_path();
    }

    fn process_item(&mut self, item: syn::Item) {
        match item {
            syn::Item::Const(item) => self.process_item_const(item),
            syn::Item::Enum(item) => self.process_item_enum(item),
            syn::Item::Fn(item) => self.process_item_fn(item),
            // syn::Item::ForeignMod(item) => todo!(),
            syn::Item::Impl(item) => self.process_item_impl(item),
            // syn::Item::Macro(item) => todo!(),
            syn::Item::Mod(item) => self.process_item_mod(item),
            syn::Item::Static(item) => self.process_item_static(item),
            syn::Item::Struct(item) => self.process_item_struct(item),
            syn::Item::Trait(item) => self.process_item_trait(item),
            syn::Item::TraitAlias(item) => self.process_item_trait_alias(item),
            syn::Item::Type(item) => self.process_item_type(item),
            syn::Item::Union(item) => self.process_item_union(item),
            syn::Item::Use(item) => self.process_item_use(item),
            // syn::Item::Verbatim(item) => todo!(),
            _ => {}
        }
    }

    fn process_item_const(&mut self, item: syn::ItemConst) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));
    }

    fn process_item_enum(&mut self, item: syn::ItemEnum) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));

        self.push_path(item.ident.to_string());
        for variant in item.variants {
            self.push_version_from_attributes(variant.ident.to_string(), variant.attrs, true);
        }
        self.pop_path();
    }

    fn process_item_fn(&mut self, item: syn::ItemFn) {
        self.push_version_from_attributes(
            item.sig.ident.to_string(),
            item.attrs,
            is_public(item.vis),
        );
    }

    fn process_item_impl(&mut self, item: syn::ItemImpl) {
        if item.trait_.is_some() {
            // FIXME: Trait implementations.
            return;
        }

        let Some(n) = self.push_type(*item.self_ty) else {
            return;
        };

        for item in item.items {
            match item {
                syn::ImplItem::Const(item) => self.process_impl_const(item),
                syn::ImplItem::Fn(item) => self.process_impl_fn(item),
                syn::ImplItem::Type(item) => self.process_impl_type(item),
                // syn::ImplItem::Macro(item) => todo!(),
                // syn::ImplItem::Verbatim(item) => todo!(),
                _ => {}
            }
        }
        self.pop_path_n(n);
    }

    fn process_impl_const(&mut self, item: syn::ImplItemConst) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));
    }

    fn process_impl_fn(&mut self, item: syn::ImplItemFn) {
        self.push_version_from_attributes(
            item.sig.ident.to_string(),
            item.attrs,
            is_public(item.vis),
        );
    }

    fn process_impl_type(&mut self, item: syn::ImplItemType) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));
    }

    fn process_item_mod(&mut self, item: syn::ItemMod) {
        let Some((_, items)) = item.content else {
            return;
        };

        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));

        self.push_path(item.ident.to_string());
        for item in items {
            self.process_item(item);
        }
        self.pop_path();
    }

    fn process_item_static(&mut self, item: syn::ItemStatic) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));
    }

    fn process_item_struct(&mut self, item: syn::ItemStruct) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));
    }

    fn process_item_trait(&mut self, item: syn::ItemTrait) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));

        self.push_path(item.ident.to_string());
        for item in item.items {
            match item {
                syn::TraitItem::Const(item) => self.process_trait_const(item),
                syn::TraitItem::Fn(item) => self.process_trait_fn(item),
                syn::TraitItem::Type(item) => self.process_trait_type(item),
                // syn::TraitItem::Macro(item) => todo!(),
                // syn::TraitItem::Verbatim(item) => todo!(),
                _ => {}
            }
        }
        self.pop_path();
    }

    fn process_trait_const(&mut self, item: syn::TraitItemConst) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, true);
    }

    fn process_trait_fn(&mut self, item: syn::TraitItemFn) {
        self.push_version_from_attributes(item.sig.ident.to_string(), item.attrs, true);
    }

    fn process_trait_type(&mut self, item: syn::TraitItemType) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, true);
    }

    fn process_item_trait_alias(&mut self, item: syn::ItemTraitAlias) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));
    }

    fn process_item_type(&mut self, item: syn::ItemType) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));
    }

    fn process_item_union(&mut self, item: syn::ItemUnion) {
        self.push_version_from_attributes(item.ident.to_string(), item.attrs, is_public(item.vis));
    }

    fn process_item_use(&mut self, item: syn::ItemUse) {
        self.process_use_tree(Vec::new(), item.tree, item.attrs, false);
    }

    fn process_use_tree(
        &mut self,
        mut relative_path: Vec<String>,
        tree: UseTree,
        attrs: Vec<syn::Attribute>,
        public: bool,
    ) {
        match tree {
            UseTree::Path(path) => {
                if path.ident != "self" {
                    relative_path.push(path.ident.to_string());
                }

                self.process_use_tree(relative_path, *path.tree, attrs.clone(), public);
            }
            UseTree::Name(name) => {
                if name.ident != "self" {
                    relative_path.push(name.ident.to_string());
                    self.aliases.push(Alias {
                        root: self.path_stack.clone().into(),
                        relative_path,
                        local: LocalAlias::Named(name.ident.to_string()),
                    });
                } else {
                    self.aliases.push(Alias {
                        root: self.path_stack.clone().into(),
                        local: LocalAlias::Named(relative_path.last().unwrap().clone()),
                        relative_path,
                    });
                }

                self.push_version_from_attributes(name.ident.to_string(), attrs, public);
            }
            UseTree::Rename(rename) => {
                if rename.ident != "self" {
                    relative_path.push(rename.ident.to_string());
                }

                self.aliases.push(Alias {
                    root: self.path_stack.clone().into(),
                    relative_path,
                    local: LocalAlias::Named(rename.rename.to_string()),
                });

                self.push_version_from_attributes(rename.ident.to_string(), attrs, public);
            }
            UseTree::Glob(_) => self.aliases.push(Alias {
                root: self.path_stack.clone().into(),
                relative_path,
                local: LocalAlias::GlobChildren,
            }),
            UseTree::Group(group) => {
                for item in group.items {
                    self.process_use_tree(relative_path.clone(), item, attrs.clone(), public);
                }
            }
        }
    }

    fn push_path(&mut self, path: String) {
        self.path_stack.push_back(path);
    }

    fn pop_path(&mut self) {
        self.path_stack.pop_back();
    }

    fn push_type(&mut self, ty: syn::Type) -> Option<usize> {
        match ty {
            syn::Type::Group(group) => self.push_type(*group.elem),
            syn::Type::Paren(paren) => self.push_type(*paren.elem),
            syn::Type::Path(path) => self.push_type_path(path.path),
            _ => None,
        }
    }

    fn push_type_path(&mut self, path: syn::Path) -> Option<usize> {
        for segment in path.segments.iter() {
            self.push_path(segment.ident.to_string());
        }

        Some(path.segments.len())
    }

    fn pop_path_n(&mut self, n: usize) {
        for _ in 0..n {
            self.pop_path();
        }
    }

    fn push_version_from_attributes(
        &mut self,
        name: String,
        mut attrs: Vec<syn::Attribute>,
        public: bool,
    ) {
        let Ok(stable) = deluxe::extract_attributes::<_, Stable>(&mut attrs) else {
            return;
        };

        self.push_path(name);
        self.push_version(stable.since, public);
        self.pop_path();
    }

    fn current_item_mut(&mut self) -> &mut VersionedItem {
        let mut current = &mut self.root;
        for section in self.path_stack.iter() {
            if section == "self" {
                continue;
            }

            current = current
                .children
                .entry(section.clone())
                .or_insert_with(|| VersionedItem::new(format!("{}::{}", current.name, section)));
        }

        current
    }

    fn push_version(&mut self, version: String, public: bool) {
        let current = self.current_item_mut();
        current.version = version;
        current.public = public;
    }

    fn resolve_path_from<'a>(
        &'a self,
        root: &'a VersionedItem,
        root_path: &[String],
        path: &[String],
    ) -> Option<&'a VersionedItem> {
        // println!("root = {root_path:?} = {}", root.name);
        // println!("resolving {path:?}...");

        let mut current = root;
        let mut last: Option<&VersionedItem> = None;

        if !root_path.is_empty() && !path.is_empty() {
            if path[0] == "crate" {
                let new_root = self.root.children.get(&root_path[0])?;
                // println!("{:?} crate shorthands to {}", &path[..1], root_path[0]);
                return self.resolve_path_from(new_root, &root_path[..1], &path[1..]);
            }

            // FIXME: HACK
            if path[0] == "alloc_crate" {
                let new_root = self.root.children.get("alloc")?;
                // println!("alloc_crate shorthands to alloc");
                return self.resolve_path_from(new_root, &["alloc".to_string()], &path[1..]);
            }
        }

        let mut root_path = root_path.to_vec();
        if !root_path.is_empty() && root_path[0] == "alloc_crate" {
            root_path[0] = "alloc".to_string();
        }

        for (i, segment) in path.iter().enumerate() {
            if segment == "super" {
                // println!("FIXME: super not supported");
                return None;
            }

            if segment == "self" {
                continue;
            }

            let next = current.children.get(segment);
            if let Some(next) = next {
                last = Some(current);
                current = next;
                continue;
            }

            // println!("not found: {i} = {segment} (full = {path:?}, we are in {root_path:?})");
            let mut path_until_here = root_path.to_vec();
            path_until_here.extend_from_slice(&path[..i]);
            let l = path_until_here.pop()?;

            // println!("lets search for aliases starting at {:?}", path_until_here);

            let aliases = self
                .aliases
                .iter()
                .filter(|alias| alias.root == path_until_here);

            // println!("candidates:");
            for alias in aliases {
                // let root = &alias.root;
                // let rel = &alias.relative_path;
                // match &alias.local {
                //     LocalAlias::Named(name) => println!("  {root:?} :: {name} -> {rel:?}"),
                //     LocalAlias::GlobChildren => println!("  {root:?} :: * -> {rel:?}"),
                // }

                match &alias.local {
                    LocalAlias::Named(name) => {
                        if name != &l {
                            continue;
                        }

                        let last = last
                            .or_else(|| self.resolve_path_from(&self.root, &[], &path_until_here))
                            .unwrap();

                        // println!(
                        //     "found named match! current = {}, last = {}",
                        //     current.name, last.name
                        // );

                        if let Some(new_root) =
                            self.resolve_path_from(last, &path_until_here, &alias.relative_path)
                        {
                            path_until_here.extend(alias.relative_path.iter().cloned());
                            // println!("re-rooting to {path_until_here:?} for {name}");
                            return self.resolve_path_from(new_root, &path_until_here, &path[i..]);
                        }

                        let new_root =
                            self.resolve_path_from(&self.root, &[], &alias.relative_path)?;
                        // println!(
                        //     "absolute re-rooting to {:?} for {name}",
                        //     &alias.relative_path
                        // );
                        return self.resolve_path_from(new_root, &alias.relative_path, &path[i..]);
                    }
                    LocalAlias::GlobChildren => {
                        let last = last
                            .or_else(|| self.resolve_path_from(&self.root, &[], &path_until_here))
                            .unwrap();

                        if let Some(new_root) =
                            self.resolve_path_from(last, &path_until_here, &alias.relative_path)
                        {
                            if new_root.children.contains_key(&l) {
                                path_until_here.extend(alias.relative_path.iter().cloned());
                                // println!("re-rooting to {path_until_here:?} for *");
                                return self.resolve_path_from(
                                    new_root,
                                    &path_until_here,
                                    &path[i..],
                                );
                            }
                        }

                        let Some(new_root) =
                            self.resolve_path_from(&self.root, &[], &alias.relative_path)
                        else {
                            continue;
                        };

                        if new_root.children.contains_key(&l) {
                            // println!("absolute re-rooting to {:?} for *", &alias.relative_path);
                            return self.resolve_path_from(
                                new_root,
                                &alias.relative_path,
                                &path[i..],
                            );
                        }
                    }
                }
            }

            return None;
        }

        Some(current)
    }

    pub fn get_version(&self, path: &[String]) -> Option<&str> {
        self.resolve_path_from(&self.root, &[], path)
            .map(|item| item.version.as_str())
    }
}

fn is_public(vis: syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

const CRATES: &[&str] = &["alloc", "core", "std"];
const CACHE_FILE: &str = "cache.json";

pub fn load_version_constructor() -> anyhow::Result<VersionConstructor> {
    if let Ok(file) = File::open(CACHE_FILE) {
        serde_json::from_reader(file).context("failed to parse context")
    } else {
        println!("creating new cache file..");
        let mut version_constructor = VersionConstructor::new();

        for crate_ in CRATES {
            version_constructor.process_file(
                crate_.to_string(),
                syn::parse_file(
                    &std::fs::read_to_string(format!("expanded-{crate_}.rs"))
                        .with_context(|| format!("failed to read expanded-{crate_}.rs"))?,
                )
                .with_context(|| format!("failed to parse {crate_} expanded source code"))?,
            );
        }

        version_constructor.aliases.push(Alias {
            root: vec![],
            relative_path: vec!["std".to_string(), "prelude".to_string(), "v1".to_string()],
            local: LocalAlias::GlobChildren,
        });

        let file = File::create(CACHE_FILE).context("failed to create cache.json")?;
        serde_json::to_writer(file, &version_constructor).context("failed to write context")?;

        Ok(version_constructor)
    }
}
