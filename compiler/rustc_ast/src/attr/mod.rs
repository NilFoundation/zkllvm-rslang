//! Functions dealing with attributes and meta items.

use crate::ast;
use crate::ast::{AttrArgs, AttrArgsEq, AttrId, AttrItem, AttrKind, AttrStyle, AttrVec, Attribute};
use crate::ast::{DelimArgs, Expr, ExprKind, LitKind, MetaItemLit};
use crate::ast::{MacDelimiter, MetaItem, MetaItemKind, NestedMetaItem, NormalAttr};
use crate::ast::{Path, PathSegment, StrStyle, DUMMY_NODE_ID};
use crate::ptr::P;
use crate::token::{self, CommentKind, Delimiter, Token};
use crate::tokenstream::{DelimSpan, Spacing, TokenTree};
use crate::tokenstream::{LazyAttrTokenStream, TokenStream};
use crate::util::comments;
use rustc_data_structures::sync::WorkerLocal;
use rustc_index::bit_set::GrowableBitSet;
use rustc_span::symbol::{sym, Ident, Symbol};
use rustc_span::Span;
use std::cell::Cell;
use std::iter;
#[cfg(debug_assertions)]
use std::ops::BitXor;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicU32, Ordering};
use thin_vec::thin_vec;

pub struct MarkedAttrs(GrowableBitSet<AttrId>);

impl MarkedAttrs {
    pub fn new() -> Self {
        // We have no idea how many attributes there will be, so just
        // initiate the vectors with 0 bits. We'll grow them as necessary.
        MarkedAttrs(GrowableBitSet::new_empty())
    }

    pub fn mark(&mut self, attr: &Attribute) {
        self.0.insert(attr.id);
    }

    pub fn is_marked(&self, attr: &Attribute) -> bool {
        self.0.contains(attr.id)
    }
}

impl NestedMetaItem {
    /// Returns the `MetaItem` if `self` is a `NestedMetaItem::MetaItem`.
    pub fn meta_item(&self) -> Option<&MetaItem> {
        match self {
            NestedMetaItem::MetaItem(item) => Some(item),
            _ => None,
        }
    }

    /// Returns the `MetaItemLit` if `self` is a `NestedMetaItem::Literal`s.
    pub fn lit(&self) -> Option<&MetaItemLit> {
        match self {
            NestedMetaItem::Lit(lit) => Some(lit),
            _ => None,
        }
    }

    /// Returns `true` if this list item is a MetaItem with a name of `name`.
    pub fn has_name(&self, name: Symbol) -> bool {
        self.meta_item().map_or(false, |meta_item| meta_item.has_name(name))
    }

    /// For a single-segment meta item, returns its name; otherwise, returns `None`.
    pub fn ident(&self) -> Option<Ident> {
        self.meta_item().and_then(|meta_item| meta_item.ident())
    }
    pub fn name_or_empty(&self) -> Symbol {
        self.ident().unwrap_or_else(Ident::empty).name
    }

    /// Gets the string value if `self` is a `MetaItem` and the `MetaItem` is a
    /// `MetaItemKind::NameValue` variant containing a string, otherwise `None`.
    pub fn value_str(&self) -> Option<Symbol> {
        self.meta_item().and_then(|meta_item| meta_item.value_str())
    }

    /// Returns a name and single literal value tuple of the `MetaItem`.
    pub fn name_value_literal(&self) -> Option<(Symbol, &MetaItemLit)> {
        self.meta_item().and_then(|meta_item| {
            meta_item.meta_item_list().and_then(|meta_item_list| {
                if meta_item_list.len() == 1
                    && let Some(ident) = meta_item.ident()
                    && let Some(lit) = meta_item_list[0].lit()
                {
                    return Some((ident.name, lit));
                }
                None
            })
        })
    }

    /// Gets a list of inner meta items from a list `MetaItem` type.
    pub fn meta_item_list(&self) -> Option<&[NestedMetaItem]> {
        self.meta_item().and_then(|meta_item| meta_item.meta_item_list())
    }

    /// Returns `true` if the variant is `MetaItem`.
    pub fn is_meta_item(&self) -> bool {
        self.meta_item().is_some()
    }

    /// Returns `true` if `self` is a `MetaItem` and the meta item is a word.
    pub fn is_word(&self) -> bool {
        self.meta_item().map_or(false, |meta_item| meta_item.is_word())
    }

    /// See [`MetaItem::name_value_literal_span`].
    pub fn name_value_literal_span(&self) -> Option<Span> {
        self.meta_item()?.name_value_literal_span()
    }
}

impl Attribute {
    #[inline]
    pub fn has_name(&self, name: Symbol) -> bool {
        match &self.kind {
            AttrKind::Normal(normal) => normal.item.path == name,
            AttrKind::DocComment(..) => false,
        }
    }

    /// For a single-segment attribute, returns its name; otherwise, returns `None`.
    pub fn ident(&self) -> Option<Ident> {
        match &self.kind {
            AttrKind::Normal(normal) => {
                if let [ident] = &*normal.item.path.segments {
                    Some(ident.ident)
                } else {
                    None
                }
            }
            AttrKind::DocComment(..) => None,
        }
    }
    pub fn name_or_empty(&self) -> Symbol {
        self.ident().unwrap_or_else(Ident::empty).name
    }

    pub fn value_str(&self) -> Option<Symbol> {
        match &self.kind {
            AttrKind::Normal(normal) => normal.item.meta_kind().and_then(|kind| kind.value_str()),
            AttrKind::DocComment(..) => None,
        }
    }

    pub fn meta_item_list(&self) -> Option<Vec<NestedMetaItem>> {
        match &self.kind {
            AttrKind::Normal(normal) => match normal.item.meta_kind() {
                Some(MetaItemKind::List(list)) => Some(list),
                _ => None,
            },
            AttrKind::DocComment(..) => None,
        }
    }

    pub fn is_word(&self) -> bool {
        if let AttrKind::Normal(normal) = &self.kind {
            matches!(normal.item.args, AttrArgs::Empty)
        } else {
            false
        }
    }
}

impl MetaItem {
    /// For a single-segment meta item, returns its name; otherwise, returns `None`.
    pub fn ident(&self) -> Option<Ident> {
        if self.path.segments.len() == 1 { Some(self.path.segments[0].ident) } else { None }
    }
    pub fn name_or_empty(&self) -> Symbol {
        self.ident().unwrap_or_else(Ident::empty).name
    }

    /// ```text
    /// Example:
    ///     #[attribute(name = "value")]
    ///                 ^^^^^^^^^^^^^^
    /// ```
    pub fn name_value_literal(&self) -> Option<&MetaItemLit> {
        match &self.kind {
            MetaItemKind::NameValue(v) => Some(v),
            _ => None,
        }
    }

    pub fn value_str(&self) -> Option<Symbol> {
        self.kind.value_str()
    }

    pub fn meta_item_list(&self) -> Option<&[NestedMetaItem]> {
        match &self.kind {
            MetaItemKind::List(l) => Some(&**l),
            _ => None,
        }
    }

    pub fn is_word(&self) -> bool {
        matches!(self.kind, MetaItemKind::Word)
    }

    pub fn has_name(&self, name: Symbol) -> bool {
        self.path == name
    }

    /// This is used in case you want the value span instead of the whole attribute. Example:
    ///
    /// ```text
    /// #[doc(alias = "foo")]
    /// ```
    ///
    /// In here, it'll return a span for `"foo"`.
    pub fn name_value_literal_span(&self) -> Option<Span> {
        Some(self.name_value_literal()?.span)
    }
}

impl AttrItem {
    pub fn span(&self) -> Span {
        self.args.span().map_or(self.path.span, |args_span| self.path.span.to(args_span))
    }

    pub fn meta(&self, span: Span) -> Option<MetaItem> {
        Some(MetaItem { path: self.path.clone(), kind: self.meta_kind()?, span })
    }

    pub fn meta_kind(&self) -> Option<MetaItemKind> {
        MetaItemKind::from_attr_args(&self.args)
    }
}

impl Attribute {
    /// Returns `true` if it is a sugared doc comment (`///` or `//!` for example).
    /// So `#[doc = "doc"]` (which is a doc comment) and `#[doc(...)]` (which is not
    /// a doc comment) will return `false`.
    pub fn is_doc_comment(&self) -> bool {
        match self.kind {
            AttrKind::Normal(..) => false,
            AttrKind::DocComment(..) => true,
        }
    }

    /// Returns the documentation and its kind if this is a doc comment or a sugared doc comment.
    /// * `///doc` returns `Some(("doc", CommentKind::Line))`.
    /// * `/** doc */` returns `Some(("doc", CommentKind::Block))`.
    /// * `#[doc = "doc"]` returns `Some(("doc", CommentKind::Line))`.
    /// * `#[doc(...)]` returns `None`.
    pub fn doc_str_and_comment_kind(&self) -> Option<(Symbol, CommentKind)> {
        match self.kind {
            AttrKind::DocComment(kind, data) => Some((data, kind)),
            AttrKind::Normal(ref normal) if normal.item.path == sym::doc => normal
                .item
                .meta_kind()
                .and_then(|kind| kind.value_str())
                .map(|data| (data, CommentKind::Line)),
            _ => None,
        }
    }

    /// Returns the documentation if this is a doc comment or a sugared doc comment.
    /// * `///doc` returns `Some("doc")`.
    /// * `#[doc = "doc"]` returns `Some("doc")`.
    /// * `#[doc(...)]` returns `None`.
    pub fn doc_str(&self) -> Option<Symbol> {
        match &self.kind {
            AttrKind::DocComment(.., data) => Some(*data),
            AttrKind::Normal(normal) if normal.item.path == sym::doc => {
                normal.item.meta_kind().and_then(|kind| kind.value_str())
            }
            _ => None,
        }
    }

    pub fn may_have_doc_links(&self) -> bool {
        self.doc_str().map_or(false, |s| comments::may_have_doc_links(s.as_str()))
    }

    pub fn get_normal_item(&self) -> &AttrItem {
        match &self.kind {
            AttrKind::Normal(normal) => &normal.item,
            AttrKind::DocComment(..) => panic!("unexpected doc comment"),
        }
    }

    pub fn unwrap_normal_item(self) -> AttrItem {
        match self.kind {
            AttrKind::Normal(normal) => normal.into_inner().item,
            AttrKind::DocComment(..) => panic!("unexpected doc comment"),
        }
    }

    /// Extracts the MetaItem from inside this Attribute.
    pub fn meta(&self) -> Option<MetaItem> {
        match &self.kind {
            AttrKind::Normal(normal) => normal.item.meta(self.span),
            AttrKind::DocComment(..) => None,
        }
    }

    pub fn meta_kind(&self) -> Option<MetaItemKind> {
        match &self.kind {
            AttrKind::Normal(normal) => normal.item.meta_kind(),
            AttrKind::DocComment(..) => None,
        }
    }

    pub fn tokens(&self) -> TokenStream {
        match &self.kind {
            AttrKind::Normal(normal) => normal
                .tokens
                .as_ref()
                .unwrap_or_else(|| panic!("attribute is missing tokens: {:?}", self))
                .to_attr_token_stream()
                .to_tokenstream(),
            &AttrKind::DocComment(comment_kind, data) => TokenStream::new(vec![TokenTree::Token(
                Token::new(token::DocComment(comment_kind, self.style, data), self.span),
                Spacing::Alone,
            )]),
        }
    }
}

/* Constructors */

pub fn mk_name_value_item_str(ident: Ident, str: Symbol, str_span: Span) -> MetaItem {
    mk_name_value_item(ident, LitKind::Str(str, ast::StrStyle::Cooked), str_span)
}

pub fn mk_name_value_item(ident: Ident, kind: LitKind, lit_span: Span) -> MetaItem {
    let lit = MetaItemLit { token_lit: kind.to_token_lit(), kind, span: lit_span };
    let span = ident.span.to(lit_span);
    MetaItem { path: Path::from_ident(ident), kind: MetaItemKind::NameValue(lit), span }
}

pub struct AttrIdGenerator(WorkerLocal<Cell<u32>>);

#[cfg(debug_assertions)]
static MAX_ATTR_ID: AtomicU32 = AtomicU32::new(u32::MAX);

impl AttrIdGenerator {
    pub fn new() -> Self {
        // We use `(index as u32).reverse_bits()` to initialize the
        // starting value of AttrId in each worker thread.
        // The `index` is the index of the worker thread.
        // This ensures that the AttrId generated in each thread is unique.
        AttrIdGenerator(WorkerLocal::new(|index| {
            let index: u32 = index.try_into().unwrap();

            #[cfg(debug_assertions)]
            {
                let max_id = ((index + 1).next_power_of_two() - 1).bitxor(u32::MAX).reverse_bits();
                MAX_ATTR_ID.fetch_min(max_id, Ordering::Release);
            }

            Cell::new(index.reverse_bits())
        }))
    }

    pub fn mk_attr_id(&self) -> AttrId {
        let id = self.0.get();

        // Ensure the assigned attr_id does not overlap the bits
        // representing the number of threads.
        #[cfg(debug_assertions)]
        assert!(id <= MAX_ATTR_ID.load(Ordering::Acquire));

        self.0.set(id + 1);
        AttrId::from_u32(id)
    }
}

pub fn mk_attr(
    g: &AttrIdGenerator,
    style: AttrStyle,
    path: Path,
    args: AttrArgs,
    span: Span,
) -> Attribute {
    mk_attr_from_item(g, AttrItem { path, args, tokens: None }, None, style, span)
}

pub fn mk_attr_from_item(
    g: &AttrIdGenerator,
    item: AttrItem,
    tokens: Option<LazyAttrTokenStream>,
    style: AttrStyle,
    span: Span,
) -> Attribute {
    Attribute {
        kind: AttrKind::Normal(P(NormalAttr { item, tokens })),
        id: g.mk_attr_id(),
        style,
        span,
    }
}

pub fn mk_attr_word(g: &AttrIdGenerator, style: AttrStyle, name: Symbol, span: Span) -> Attribute {
    let path = Path::from_ident(Ident::new(name, span));
    let args = AttrArgs::Empty;
    mk_attr(g, style, path, args, span)
}

pub fn mk_attr_name_value_str(
    g: &AttrIdGenerator,
    style: AttrStyle,
    name: Symbol,
    val: Symbol,
    span: Span,
) -> Attribute {
    let lit = LitKind::Str(val, StrStyle::Cooked).to_token_lit();
    let expr = P(Expr {
        id: DUMMY_NODE_ID,
        kind: ExprKind::Lit(lit),
        span,
        attrs: AttrVec::new(),
        tokens: None,
    });
    let path = Path::from_ident(Ident::new(name, span));
    let args = AttrArgs::Eq(span, AttrArgsEq::Ast(expr));
    mk_attr(g, style, path, args, span)
}

pub fn mk_attr_nested_word(
    g: &AttrIdGenerator,
    style: AttrStyle,
    outer: Symbol,
    inner: Symbol,
    span: Span,
) -> Attribute {
    let inner_tokens = TokenStream::new(vec![TokenTree::Token(
        Token::from_ast_ident(Ident::new(inner, span)),
        Spacing::Alone,
    )]);
    let outer_ident = Ident::new(outer, span);
    let path = Path::from_ident(outer_ident);
    let attr_args = AttrArgs::Delimited(DelimArgs {
        dspan: DelimSpan::from_single(span),
        delim: MacDelimiter::Parenthesis,
        tokens: inner_tokens,
    });
    mk_attr(g, style, path, attr_args, span)
}

pub fn mk_doc_comment(
    g: &AttrIdGenerator,
    comment_kind: CommentKind,
    style: AttrStyle,
    data: Symbol,
    span: Span,
) -> Attribute {
    Attribute { kind: AttrKind::DocComment(comment_kind, data), id: g.mk_attr_id(), style, span }
}

pub fn list_contains_name(items: &[NestedMetaItem], name: Symbol) -> bool {
    items.iter().any(|item| item.has_name(name))
}

impl MetaItem {
    fn from_tokens<I>(tokens: &mut iter::Peekable<I>) -> Option<MetaItem>
    where
        I: Iterator<Item = TokenTree>,
    {
        // FIXME: Share code with `parse_path`.
        let path = match tokens.next().map(TokenTree::uninterpolate) {
            Some(TokenTree::Token(
                Token { kind: kind @ (token::Ident(..) | token::ModSep), span },
                _,
            )) => 'arm: {
                let mut segments = if let token::Ident(name, _) = kind {
                    if let Some(TokenTree::Token(Token { kind: token::ModSep, .. }, _)) =
                        tokens.peek()
                    {
                        tokens.next();
                        thin_vec![PathSegment::from_ident(Ident::new(name, span))]
                    } else {
                        break 'arm Path::from_ident(Ident::new(name, span));
                    }
                } else {
                    thin_vec![PathSegment::path_root(span)]
                };
                loop {
                    if let Some(TokenTree::Token(Token { kind: token::Ident(name, _), span }, _)) =
                        tokens.next().map(TokenTree::uninterpolate)
                    {
                        segments.push(PathSegment::from_ident(Ident::new(name, span)));
                    } else {
                        return None;
                    }
                    if let Some(TokenTree::Token(Token { kind: token::ModSep, .. }, _)) =
                        tokens.peek()
                    {
                        tokens.next();
                    } else {
                        break;
                    }
                }
                let span = span.with_hi(segments.last().unwrap().ident.span.hi());
                Path { span, segments, tokens: None }
            }
            Some(TokenTree::Token(Token { kind: token::Interpolated(nt), .. }, _)) => match &*nt {
                token::Nonterminal::NtMeta(item) => return item.meta(item.path.span),
                token::Nonterminal::NtPath(path) => (**path).clone(),
                _ => return None,
            },
            _ => return None,
        };
        let list_closing_paren_pos = tokens.peek().map(|tt| tt.span().hi());
        let kind = MetaItemKind::from_tokens(tokens)?;
        let hi = match &kind {
            MetaItemKind::NameValue(lit) => lit.span.hi(),
            MetaItemKind::List(..) => list_closing_paren_pos.unwrap_or(path.span.hi()),
            _ => path.span.hi(),
        };
        let span = path.span.with_hi(hi);
        Some(MetaItem { path, kind, span })
    }
}

impl MetaItemKind {
    pub fn value_str(&self) -> Option<Symbol> {
        match self {
            MetaItemKind::NameValue(v) => match v.kind {
                LitKind::Str(s, _) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    fn list_from_tokens(tokens: TokenStream) -> Option<MetaItemKind> {
        let mut tokens = tokens.into_trees().peekable();
        let mut result = Vec::new();
        while tokens.peek().is_some() {
            let item = NestedMetaItem::from_tokens(&mut tokens)?;
            result.push(item);
            match tokens.next() {
                None | Some(TokenTree::Token(Token { kind: token::Comma, .. }, _)) => {}
                _ => return None,
            }
        }
        Some(MetaItemKind::List(result))
    }

    fn name_value_from_tokens(
        tokens: &mut impl Iterator<Item = TokenTree>,
    ) -> Option<MetaItemKind> {
        match tokens.next() {
            Some(TokenTree::Delimited(_, Delimiter::Invisible, inner_tokens)) => {
                MetaItemKind::name_value_from_tokens(&mut inner_tokens.into_trees())
            }
            Some(TokenTree::Token(token, _)) => {
                MetaItemLit::from_token(&token).map(MetaItemKind::NameValue)
            }
            _ => None,
        }
    }

    fn from_attr_args(args: &AttrArgs) -> Option<MetaItemKind> {
        match args {
            AttrArgs::Empty => Some(MetaItemKind::Word),
            AttrArgs::Delimited(DelimArgs {
                dspan: _,
                delim: MacDelimiter::Parenthesis,
                tokens,
            }) => MetaItemKind::list_from_tokens(tokens.clone()),
            AttrArgs::Delimited(..) => None,
            AttrArgs::Eq(_, AttrArgsEq::Ast(expr)) => match expr.kind {
                ExprKind::Lit(token_lit) => {
                    // Turn failures to `None`, we'll get parse errors elsewhere.
                    MetaItemLit::from_token_lit(token_lit, expr.span)
                        .ok()
                        .map(|lit| MetaItemKind::NameValue(lit))
                }
                _ => None,
            },
            AttrArgs::Eq(_, AttrArgsEq::Hir(lit)) => Some(MetaItemKind::NameValue(lit.clone())),
        }
    }

    fn from_tokens(
        tokens: &mut iter::Peekable<impl Iterator<Item = TokenTree>>,
    ) -> Option<MetaItemKind> {
        match tokens.peek() {
            Some(TokenTree::Delimited(_, Delimiter::Parenthesis, inner_tokens)) => {
                let inner_tokens = inner_tokens.clone();
                tokens.next();
                MetaItemKind::list_from_tokens(inner_tokens)
            }
            Some(TokenTree::Delimited(..)) => None,
            Some(TokenTree::Token(Token { kind: token::Eq, .. }, _)) => {
                tokens.next();
                MetaItemKind::name_value_from_tokens(tokens)
            }
            _ => Some(MetaItemKind::Word),
        }
    }
}

impl NestedMetaItem {
    pub fn span(&self) -> Span {
        match self {
            NestedMetaItem::MetaItem(item) => item.span,
            NestedMetaItem::Lit(lit) => lit.span,
        }
    }

    fn from_tokens<I>(tokens: &mut iter::Peekable<I>) -> Option<NestedMetaItem>
    where
        I: Iterator<Item = TokenTree>,
    {
        match tokens.peek() {
            Some(TokenTree::Token(token, _))
                if let Some(lit) = MetaItemLit::from_token(token) =>
            {
                tokens.next();
                return Some(NestedMetaItem::Lit(lit));
            }
            Some(TokenTree::Delimited(_, Delimiter::Invisible, inner_tokens)) => {
                let inner_tokens = inner_tokens.clone();
                tokens.next();
                return NestedMetaItem::from_tokens(&mut inner_tokens.into_trees().peekable());
            }
            _ => {}
        }
        MetaItem::from_tokens(tokens).map(NestedMetaItem::MetaItem)
    }
}
