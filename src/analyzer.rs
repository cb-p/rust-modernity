use std::collections::HashMap;

use crate::std_versions::VersionConstructor;

pub struct VersionAnalyzer<'a> {
    version_constructor: &'a VersionConstructor,

    path: Vec<String>,
    nested_unsafe: usize,

    pub version_counts: HashMap<String, usize>,
    pub total_exprs: usize,
    pub unsafe_exprs: usize,
}

impl<'a> VersionAnalyzer<'a> {
    pub fn new(version_constructor: &'a VersionConstructor) -> VersionAnalyzer<'a> {
        VersionAnalyzer {
            version_constructor,

            path: Vec::new(),
            nested_unsafe: 0,

            version_counts: HashMap::new(),
            total_exprs: 0,
            unsafe_exprs: 0,
        }
    }

    pub fn process_file(&mut self, file: syn::File) {
        for item in file.items {
            self.process_item(item);
        }
    }

    fn process_item(&mut self, item: syn::Item) {
        match item {
            syn::Item::Const(item) => self.process_item_const(item),
            syn::Item::Enum(item) => self.process_item_enum(item),
            // syn::Item::ExternCrate(item) => {},
            syn::Item::Fn(item) => self.process_item_fn(item),
            // syn::Item::ForeignMod(item) => {},
            syn::Item::Impl(item) => self.process_item_impl(item),
            // syn::Item::Macro(item) => {},
            syn::Item::Mod(item) => self.process_item_mod(item),
            syn::Item::Static(item) => self.process_item_static(item),
            syn::Item::Struct(item) => self.process_item_struct(item),
            syn::Item::Trait(item) => self.process_item_trait(item),
            // syn::Item::TraitAlias(item) => {},
            syn::Item::Type(item) => self.process_item_type(item),
            // syn::Item::Union(item) => {},
            syn::Item::Use(item) => self.process_item_use(item),
            // syn::Item::Verbatim(item) => {},
            _ => {}
        }
    }

    fn process_item_type(&mut self, item: syn::ItemType) {
        self.process_type(*item.ty);
    }

    fn process_item_trait(&mut self, item: syn::ItemTrait) {
        for item in item.items {
            match item {
                syn::TraitItem::Const(const_) => {
                    self.process_type(const_.ty);
                    if let Some((_, expr)) = const_.default {
                        self.process_expr(expr);
                    }
                }
                syn::TraitItem::Fn(fn_) => {
                    self.process_sig(fn_.sig);
                    if let Some(block) = fn_.default {
                        self.process_block(block);
                    }
                }
                syn::TraitItem::Type(ty) => {
                    if let Some((_, ty)) = ty.default {
                        self.process_type(ty);
                    }
                }
                // syn::TraitItem::Macro(_) => todo!(),
                // syn::TraitItem::Verbatim(_) => todo!(),
                _ => {}
            }
        }
    }

    fn process_item_struct(&mut self, item: syn::ItemStruct) {
        match item.fields {
            syn::Fields::Named(named) => {
                for field in named.named {
                    self.process_type(field.ty);
                }
            }
            syn::Fields::Unnamed(unnamed) => {
                for field in unnamed.unnamed {
                    self.process_type(field.ty);
                }
            }
            _ => {}
        }
    }

    fn process_item_static(&mut self, item: syn::ItemStatic) {
        self.process_type(*item.ty);
        self.process_expr(*item.expr);
    }

    fn process_item_const(&mut self, item: syn::ItemConst) {
        self.process_type(*item.ty);
        self.process_expr(*item.expr);
    }

    fn process_item_enum(&mut self, item: syn::ItemEnum) {
        for variant in item.variants {
            if let Some((_, expr)) = variant.discriminant {
                self.process_expr(expr);
            }

            match variant.fields {
                syn::Fields::Named(named) => {
                    for field in named.named {
                        self.process_type(field.ty);
                    }
                }
                syn::Fields::Unnamed(unnamed) => {
                    for field in unnamed.unnamed {
                        self.process_type(field.ty);
                    }
                }
                _ => {}
            }
        }
    }

    fn process_item_impl(&mut self, item: syn::ItemImpl) {
        // Can't implement for standard libary types, not needed.
        // self.process_type(*item.self_ty);
        if let Some((_, path, _)) = item.trait_ {
            self.process_path(path);
        }

        for item in item.items {
            self.process_impl_item(item);
        }
    }

    fn process_impl_item(&mut self, item: syn::ImplItem) {
        match item {
            syn::ImplItem::Const(const_) => {
                self.process_type(const_.ty);
                self.process_expr(const_.expr);
            }
            syn::ImplItem::Fn(fun) => {
                if fun.sig.unsafety.is_some() {
                    self.nested_unsafe += 1;
                    self.process_sig(fun.sig);
                    self.process_block(fun.block);
                    self.nested_unsafe -= 1;
                } else {
                    self.process_sig(fun.sig);
                    self.process_block(fun.block);
                }
            }
            syn::ImplItem::Type(ty) => {
                self.process_type(ty.ty);
            }
            // syn::ImplItem::Macro(_) => todo!(),
            // syn::ImplItem::Verbatim(_) => todo!(),
            _ => {}
        }
    }

    fn process_item_fn(&mut self, item: syn::ItemFn) {
        if item.sig.unsafety.is_some() {
            self.nested_unsafe += 1;
            self.process_sig(item.sig);
            self.process_block(*item.block);
            self.nested_unsafe -= 1;
        } else {
            self.process_sig(item.sig);
            self.process_block(*item.block);
        }
    }

    fn process_sig(&mut self, sig: syn::Signature) {
        for arg in sig.inputs {
            if let syn::FnArg::Typed(typed) = arg {
                self.process_type(*typed.ty);
            }
        }

        if let syn::ReturnType::Type(_, ty) = sig.output {
            self.process_type(*ty);
        }
    }

    fn process_block(&mut self, block: syn::Block) {
        for stmt in block.stmts {
            self.process_statement(stmt);
        }
    }

    fn process_statement(&mut self, stmt: syn::Stmt) {
        match stmt {
            syn::Stmt::Local(local) => {
                // FIXME: Process pattern.
                if let Some(init) = local.init {
                    self.process_expr(*init.expr);
                    if let Some((_, expr)) = init.diverge {
                        self.process_expr(*expr);
                    }
                }
            }
            syn::Stmt::Item(item) => self.process_item(item),
            syn::Stmt::Expr(expr, _) => self.process_expr(expr),
            syn::Stmt::Macro(_) => {}
        }
    }

    fn process_expr(&mut self, expr: syn::Expr) {
        self.count_expr();

        match expr {
            syn::Expr::Array(array) => {
                for expr in array.elems {
                    self.process_expr(expr);
                }
            }
            syn::Expr::Assign(assign) => {
                self.process_expr(*assign.left);
                self.process_expr(*assign.right);
            }
            syn::Expr::Async(asyn) => self.process_block(asyn.block),
            syn::Expr::Await(await_) => self.process_expr(*await_.base),
            syn::Expr::Binary(binary) => {
                self.process_expr(*binary.left);
                self.process_expr(*binary.right);
            }
            syn::Expr::Block(block) => self.process_block(block.block),
            syn::Expr::Break(break_) => {
                if let Some(expr) = break_.expr {
                    self.process_expr(*expr);
                }
            }
            syn::Expr::Call(call) => {
                self.process_expr(*call.func);
                for expr in call.args {
                    self.process_expr(expr);
                }
            }
            syn::Expr::Cast(cast) => {
                self.process_expr(*cast.expr);
                self.process_type(*cast.ty);
            }
            syn::Expr::Closure(closure) => {
                // FIXME: Process input pats.
                if let syn::ReturnType::Type(_, ty) = closure.output {
                    self.process_type(*ty);
                }

                self.process_expr(*closure.body);
            }
            syn::Expr::Const(const_) => {
                self.process_block(const_.block);
            }
            // syn::Expr::Continue(_) => todo!(),
            // syn::Expr::Field(_) => todo!(), FIXME: fields.
            syn::Expr::ForLoop(for_) => {
                self.process_expr(*for_.expr);
                self.process_block(for_.body);
            }
            syn::Expr::Group(group) => self.process_expr(*group.expr),
            syn::Expr::If(if_) => {
                self.process_expr(*if_.cond);
                self.process_block(if_.then_branch);
                if let Some((_, expr)) = if_.else_branch {
                    self.process_expr(*expr);
                }
            }
            syn::Expr::Index(index) => {
                self.process_expr(*index.expr);
                self.process_expr(*index.index);
            }
            // syn::Expr::Infer(_) => todo!(),
            syn::Expr::Let(let_) => self.process_expr(*let_.expr),
            // syn::Expr::Lit(_) => todo!(),
            syn::Expr::Loop(loop_) => self.process_block(loop_.body),
            // syn::Expr::Macro(_) => todo!(),
            syn::Expr::Match(match_) => {
                self.process_expr(*match_.expr);
                for arm in match_.arms {
                    if let Some((_, expr)) = arm.guard {
                        self.process_expr(*expr);
                    }
                    self.process_expr(*arm.body);
                }
            }
            syn::Expr::MethodCall(call) => {
                self.process_expr(*call.receiver);
                for expr in call.args {
                    self.process_expr(expr);
                }
            }
            syn::Expr::Paren(paren) => self.process_expr(*paren.expr),
            syn::Expr::Path(path) => {
                self.process_path(path.path);
            }
            syn::Expr::Range(range) => {
                if let Some(expr) = range.start {
                    self.process_expr(*expr);
                }

                if let Some(expr) = range.end {
                    self.process_expr(*expr);
                }
            }
            syn::Expr::Reference(ref_) => self.process_expr(*ref_.expr),
            syn::Expr::Repeat(repeat) => {
                self.process_expr(*repeat.expr);
                self.process_expr(*repeat.len);
            }
            syn::Expr::Return(ret) => {
                if let Some(expr) = ret.expr {
                    self.process_expr(*expr);
                }
            }
            syn::Expr::Struct(struct_) => {
                self.process_path(struct_.path);
                for field in struct_.fields {
                    self.process_expr(field.expr);
                }
            }
            syn::Expr::Try(try_) => self.process_expr(*try_.expr),
            syn::Expr::TryBlock(try_block) => self.process_block(try_block.block),
            syn::Expr::Tuple(tuple) => {
                for expr in tuple.elems {
                    self.process_expr(expr);
                }
            }
            syn::Expr::Unary(unary) => self.process_expr(*unary.expr),
            syn::Expr::Unsafe(unsafe_) => {
                self.nested_unsafe += 1;
                self.process_block(unsafe_.block);
                self.nested_unsafe -= 1;
            }
            // syn::Expr::Verbatim(_) => todo!(),
            syn::Expr::While(while_) => {
                self.process_expr(*while_.cond);
                self.process_block(while_.body);
            }
            syn::Expr::Yield(yield_) => {
                if let Some(expr) = yield_.expr {
                    self.process_expr(*expr);
                }
            }
            _ => {}
        }
    }

    fn process_path(&mut self, path: syn::Path) {
        // FIXME: Process imports.

        let mut relative_path = Vec::new();
        for segment in path.segments {
            relative_path.push(segment.ident.to_string());
        }

        self.process_relative_path(&relative_path);
    }

    fn process_relative_path(&mut self, relative_path: &[String]) {
        if let Some(version) = self.version_constructor.get_version(relative_path) {
            self.count_version(version);
        } else {
            // FIXME: This does not work without us keeping track of all imports in here too.

            // let mut full_path = Vec::with_capacity(self.path.len() + relative_path.len());
            // full_path.extend_from_slice(&self.path);
            // full_path.extend_from_slice(relative_path);

            // println!("checking full path... {full_path:?}");

            // if let Some(version) = self.version_constructor.get_version(&full_path) {
            //     self.count_version(version);
            // }
        }
    }

    fn process_item_mod(&mut self, item: syn::ItemMod) {
        let Some((_, items)) = item.content else {
            return;
        };

        self.path.push(item.ident.to_string());

        for item in items {
            self.process_item(item);
        }

        self.path.pop().unwrap();
    }

    fn process_item_use(&mut self, item: syn::ItemUse) {
        self.process_use_tree(Vec::new(), item.tree);
    }

    fn process_use_tree(&mut self, mut relative_path: Vec<String>, tree: syn::UseTree) {
        match tree {
            syn::UseTree::Path(path) => {
                relative_path.push(path.ident.to_string());
                self.process_use_tree(relative_path, *path.tree);
            }
            syn::UseTree::Name(name) => {
                relative_path.push(name.ident.to_string());
                self.process_relative_path(&relative_path);
            }
            syn::UseTree::Rename(rename) => {
                relative_path.push(rename.ident.to_string());
                self.process_relative_path(&relative_path);
            }
            syn::UseTree::Glob(_) => {
                // FIXME: Support globs
            }
            syn::UseTree::Group(group) => {
                for item in group.items {
                    self.process_use_tree(relative_path.clone(), item);
                }
            }
        }
    }

    // FIXME: Process imports
    fn process_type(&mut self, ty: syn::Type) {
        match ty {
            syn::Type::Array(array) => self.process_type(*array.elem),
            syn::Type::BareFn(fun) => {
                if let syn::ReturnType::Type(_, ty) = fun.output {
                    self.process_type(*ty);
                }

                for arg in fun.inputs {
                    self.process_type(arg.ty);
                }
            }
            syn::Type::Group(group) => self.process_type(*group.elem),
            // syn::Type::ImplTrait(imp) => {}
            // syn::Type::Infer(_) => todo!(),
            // syn::Type::Macro(_) => todo!(),
            // syn::Type::Never(_) => todo!(),
            syn::Type::Paren(paren) => self.process_type(*paren.elem),
            syn::Type::Path(path) => self.process_path(path.path),
            syn::Type::Ptr(ptr) => self.process_type(*ptr.elem),
            syn::Type::Reference(ref_) => self.process_type(*ref_.elem),
            syn::Type::Slice(slice) => self.process_type(*slice.elem),
            // syn::Type::TraitObject(_) => todo!(),
            syn::Type::Tuple(tuple) => {
                for elem in tuple.elems {
                    self.process_type(elem);
                }
            }
            // syn::Type::Verbatim(_) => todo!(),
            _ => {}
        }
    }

    fn count_version(&mut self, version: &str) {
        if let Some(count) = self.version_counts.get_mut(version) {
            *count += 1;
        } else {
            self.version_counts.insert(version.to_string(), 1);
        }
    }

    fn count_expr(&mut self) {
        self.total_exprs += 1;

        if self.nested_unsafe > 0 {
            self.unsafe_exprs += 1;
        }
    }
}
