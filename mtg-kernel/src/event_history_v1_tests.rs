use super::*;
use crate::ids::ObjectId;

fn tap(id: u32) -> CommittedEvent {
    CommittedEvent::Tap {
        object: ObjectId(id),
    }
}

fn upkeep() -> CommittedEvent {
    CommittedEvent::UpkeepBegan {
        player: crate::ids::PlayerId(0),
    }
}

fn counted(history: &EventHistoryV1) -> bool {
    !history.stale && history.counts == EventHistoryV1::from(history.events.clone()).counts
}

#[test]
fn counts_follow_every_mutation() {
    let mut history = EventHistoryV1::new();
    assert!(!history.has_any(&[EventKindV1::Tap]));
    history.push(tap(1));
    history.extend([tap(2), tap(3), upkeep()]);
    history.insert(1, tap(4));
    assert!(counted(&history) && history.counts[EventKindV1::Tap as usize] == 4);
    history.remove(0);
    history.retain(|event| *event != tap(2));
    history.pop();
    assert!(counted(&history) && history.has_any(&[EventKindV1::Tap]));
    assert!(!history.has_any(&[EventKindV1::UpkeepBegan]));
    history.truncate(1);
    assert!(counted(&history) && history.counts[EventKindV1::Tap as usize] == 1);
    // A change through `DerefMut` marks the counts stale: every scan
    // runs until a counting method recounts.
    history[0] = upkeep();
    assert!(history.stale && history.has_any(&[EventKindV1::Tap]));
    history.push(tap(5));
    history.pop();
    assert!(counted(&history) && !history.has_any(&[EventKindV1::Tap]));
    for event in &mut history {
        *event = tap(6);
    }
    assert!(history.has_any(&[EventKindV1::UpkeepBegan]));
    history.truncate(5);
    assert!(counted(&history) && !history.has_any(&[EventKindV1::UpkeepBegan]));
    history.clear();
    assert!(counted(&history) && !history.has_any(&[EventKindV1::Tap]));
}

#[test]
fn records_and_digests_equal_the_plain_vec() {
    use std::collections::hash_map::DefaultHasher;
    let events = vec![tap(4), tap(9)];
    let history = EventHistoryV1::from(events.clone());
    assert_eq!(
        serde_json::to_string(&history).unwrap(),
        serde_json::to_string(&events).unwrap()
    );
    assert_eq!(format!("{history:?}"), format!("{events:?}"));
    let digest = |value: &dyn Fn(&mut DefaultHasher)| {
        let mut hasher = DefaultHasher::new();
        value(&mut hasher);
        hasher.finish()
    };
    assert_eq!(digest(&|h| history.hash(h)), digest(&|h| events.hash(h)));
    let back: EventHistoryV1 =
        serde_json::from_str(&serde_json::to_string(&events).unwrap()).unwrap();
    assert_eq!(back, history);
    assert!(counted(&back) && back.counts[EventKindV1::Tap as usize] == 2);
}

/// Every byte a value feeds a hasher, so two values can be compared on the
/// exact calls their `Hash` makes.
#[derive(Default)]
struct HashBytes(Vec<u8>);

impl Hasher for HashBytes {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

fn hash_bytes(value: &impl Hash) -> Vec<u8> {
    let mut hasher = HashBytes::default();
    value.hash(&mut hasher);
    hasher.0
}

#[test]
fn the_samples_hold_one_event_of_every_kind_in_order() {
    let samples = guard_tests_v1::every_kind_v1();
    let help = "one sample per event kind, in kind order: see data/event_history_v1/README.md";
    assert_eq!(samples.len(), EVENT_KIND_COUNT_V1, "{help}");
    for (index, sample) in samples.iter().enumerate() {
        assert_eq!(sample.kind_v1() as usize, index, "{help}: {sample:?}");
    }
}

#[test]
fn histories_of_every_kind_record_digest_compare_and_print_as_the_plain_vec() {
    let every = guard_tests_v1::every_kind_v1();
    let mut histories = vec![
        Vec::new(),
        every.clone(),
        every.iter().rev().cloned().collect(),
    ];
    histories.extend(every.iter().map(|event| vec![event.clone(), event.clone()]));
    for events in histories {
        let history = EventHistoryV1::from(events.clone());
        let record = serde_json::to_vec(&events).unwrap();
        assert_eq!(serde_json::to_vec(&history).unwrap(), record);
        assert_eq!(
            serde_json::to_vec_pretty(&history).unwrap(),
            serde_json::to_vec_pretty(&events).unwrap()
        );
        assert_eq!(hash_bytes(&history), hash_bytes(&events));
        assert_eq!(format!("{history:?}"), format!("{events:?}"));
        assert_eq!(format!("{history:#?}"), format!("{events:#?}"));
        assert_eq!(history, events);
        assert_eq!(events, history);
        assert_eq!(history, events[..]);
        assert_eq!(events[..], history);
        let back: EventHistoryV1 = serde_json::from_slice(&record).unwrap();
        assert!(back == history && counted(&back));
        let copy = history.clone();
        assert!(copy == history && counted(&copy));
        for event in &events {
            assert!(history.has_any(&[event.kind_v1()]));
        }
        let absent: Vec<EventKindV1> = every
            .iter()
            .map(CommittedEvent::kind_v1)
            .filter(|kind| events.iter().all(|event| event.kind_v1() != *kind))
            .collect();
        assert!(!history.has_any(&absent));
    }
}

/// Reads back, from the code of each guarded scan, the event kinds the scan
/// acts on, and requires its guard to list exactly those kinds. A guard
/// `history.scan_for_v1(&[..])` may skip its scan only when the scan does
/// nothing for an event of any other kind, so only these shapes are
/// accepted, with `ev` the event one step of the scan visits:
///
/// - a `for` loop over the guard (`.iter()`, `.enumerate()`) whose body
///   starts `let PAT = ev else { continue; };`, or is only statements
///   `if let PAT = ev { .. }`, or is just `match ev { .. }` where every arm
///   is a pattern of `CommittedEvent` variants except a `_` arm that does
///   nothing;
/// - the guard's `.iter()` (then `.enumerate()`, `.rev()`) into `any`,
///   `filter`, `find`, `position` or `rposition`, or into `filter_map` or
///   `find_map`, with a closure that gives `false` (`None` for the last two)
///   for any other kind: `matches!(ev, PAT)`, `match ev { .., _ => false }`,
///   `if let PAT = ev { .. } else { false }`, a body that starts
///   `let PAT = ev else { return false; };`, or for the last two
///   `matches!(ev, PAT).then_some(x)` with `x` a plain value.
///
/// The kinds are the variants those patterns name. A guard anywhere else
/// fails the audit, so a guard and its scan cannot drift apart.
mod guard_audit {
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use std::path::{Path, PathBuf};
    use syn::visit::{self, Visit};
    use syn::{Expr, ExprMethodCall, Pat, Stmt};

    type Kinds = BTreeSet<String>;

    const ADAPTERS: [&str; 7] = [
        "any",
        "filter",
        "find",
        "position",
        "rposition",
        "filter_map",
        "find_map",
    ];

    #[derive(Default)]
    struct Report {
        guards: usize,
        failures: Vec<String>,
    }

    fn audit_source(file: &str, source: &str, report: &mut Report) {
        let syntax = match syn::parse_file(source) {
            Ok(syntax) => syntax,
            Err(error) => {
                report
                    .failures
                    .push(format!("{file}: does not parse: {error}"));
                return;
            }
        };
        let mut consts = Consts::default();
        consts.visit_file(&syntax);
        let consts = consts.lists();
        let mut audit = Audit {
            file,
            consts: &consts,
            module: Vec::new(),
            function: String::new(),
            claimed: HashSet::new(),
            seen: 0,
            report,
        };
        audit.visit_file(&syntax);
        // A guard inside a macro call is not parsed, so it would escape the
        // audit: every guard written must be one the audit visited.
        let written: usize = source
            .lines()
            .map(|line| {
                line.split("//")
                    .next()
                    .unwrap_or("")
                    .matches(".scan_for_v1(")
                    .count()
            })
            .sum();
        if written != audit.seen {
            audit.report.failures.push(format!(
                "{file}: {written} guards written but {} audited; a guard inside a macro call cannot be audited",
                audit.seen
            ));
        }
    }

    /// A guard's list by name, keyed by the module that defines it.
    type Lists = BTreeMap<(Vec<String>, String), Kinds>;

    /// The kind lists a guard can name: consts defined directly in a module,
    /// under a name defined nowhere else in the file, so the name a guard in
    /// that module writes cannot mean another list.
    #[derive(Default)]
    struct Consts {
        module: Vec<String>,
        nested: usize,
        lists: Lists,
        names: BTreeMap<String, usize>,
    }

    impl Consts {
        fn define(&mut self, name: &syn::Ident) {
            *self.names.entry(name.to_string()).or_default() += 1;
        }

        fn lists(self) -> Lists {
            let names = self.names;
            self.lists
                .into_iter()
                .filter(|((_, name), _)| names[name] == 1)
                .collect()
        }
    }

    impl<'ast> Visit<'ast> for Consts {
        fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
            self.module.push(node.ident.to_string());
            visit::visit_item_mod(self, node);
            self.module.pop();
        }

        fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
            self.define(&node.ident);
            if self.nested == 0 {
                if let Ok(kinds) = kind_list(&node.expr) {
                    let key = (self.module.clone(), node.ident.to_string());
                    self.lists.insert(key, kinds);
                }
            }
            visit::visit_item_const(self, node);
        }

        fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
            self.define(&node.ident);
            visit::visit_item_static(self, node);
        }

        fn visit_impl_item_const(&mut self, node: &'ast syn::ImplItemConst) {
            self.define(&node.ident);
            visit::visit_impl_item_const(self, node);
        }

        fn visit_trait_item_const(&mut self, node: &'ast syn::TraitItemConst) {
            self.define(&node.ident);
            visit::visit_trait_item_const(self, node);
        }

        fn visit_block(&mut self, node: &'ast syn::Block) {
            self.nested += 1;
            visit::visit_block(self, node);
            self.nested -= 1;
        }
    }

    struct Audit<'a> {
        file: &'a str,
        consts: &'a Lists,
        module: Vec<String>,
        function: String,
        claimed: HashSet<*const ExprMethodCall>,
        seen: usize,
        report: &'a mut Report,
    }

    impl Audit<'_> {
        fn check(&mut self, guard: &ExprMethodCall, scan: Result<Kinds, String>) {
            self.claimed.insert(guard);
            let at = format!("{} fn {}", self.file, self.function);
            let listed = match listed(guard, &self.module, self.consts) {
                Ok(listed) => listed,
                Err(error) => {
                    self.report.failures.push(format!("{at}: {error}"));
                    return;
                }
            };
            match scan {
                Ok(kinds) if kinds == listed => self.report.guards += 1,
                Ok(kinds) => self.report.failures.push(format!(
                    "{at}: the guard lists {listed:?} but its scan acts on {kinds:?}"
                )),
                Err(shape) => self.report.failures.push(format!(
                    "{at}: the guard's scan has a shape the audit cannot read ({shape})"
                )),
            }
        }

        fn within<T>(&mut self, function: &syn::Ident, visit: impl FnOnce(&mut Self) -> T) -> T {
            let outer = std::mem::replace(&mut self.function, function.to_string());
            let result = visit(self);
            self.function = outer;
            result
        }
    }

    impl<'ast> Visit<'ast> for Audit<'_> {
        fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
            self.module.push(node.ident.to_string());
            visit::visit_item_mod(self, node);
            self.module.pop();
        }

        fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
            self.within(&node.sig.ident, |audit| visit::visit_item_fn(audit, node));
        }

        fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
            self.within(&node.sig.ident, |audit| {
                visit::visit_impl_item_fn(audit, node)
            });
        }

        fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
            self.within(&node.sig.ident, |audit| {
                visit::visit_trait_item_fn(audit, node)
            });
        }

        fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
            if let Some((guard, enumerated)) = loop_guard(&node.expr) {
                let scan = loop_kinds(&node.pat, enumerated, &node.body);
                self.check(guard, scan);
            }
            visit::visit_expr_for_loop(self, node);
        }

        fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
            if node.method == "scan_for_v1" {
                self.seen += 1;
                if !self.claimed.contains(&(node as *const ExprMethodCall)) {
                    self.report.failures.push(format!(
                        "{} fn {}: a guard outside a scan the audit can read",
                        self.file, self.function
                    ));
                }
            } else if let Some((guard, enumerated)) = adapter_guard(node) {
                let scan = adapter_kinds(node, enumerated);
                self.check(guard, scan);
            }
            visit::visit_expr_method_call(self, node);
        }
    }

    fn method(expr: &Expr) -> Option<&ExprMethodCall> {
        match expr {
            Expr::MethodCall(call) => Some(call),
            Expr::Paren(inner) => method(&inner.expr),
            _ => None,
        }
    }

    fn bare(call: &ExprMethodCall, name: &str) -> bool {
        call.method == name && call.args.is_empty()
    }

    /// The guard a `for` loop iterates: `g`, `g.iter()` or
    /// `g.iter().enumerate()`.
    fn loop_guard(expr: &Expr) -> Option<(&ExprMethodCall, bool)> {
        let mut expr = expr;
        let mut enumerated = false;
        if let Some(call) = method(expr).filter(|call| bare(call, "enumerate")) {
            expr = &call.receiver;
            enumerated = true;
        }
        if let Some(call) = method(expr).filter(|call| bare(call, "iter")) {
            expr = &call.receiver;
        } else if enumerated {
            return None;
        }
        method(expr)
            .filter(|call| call.method == "scan_for_v1")
            .map(|call| (call, enumerated))
    }

    /// The guard under an adapter call: `g.iter()`, then any of
    /// `.enumerate()` and `.rev()`, then the adapter.
    fn adapter_guard(call: &ExprMethodCall) -> Option<(&ExprMethodCall, bool)> {
        if !ADAPTERS.iter().any(|adapter| call.method == adapter) {
            return None;
        }
        let mut receiver = &*call.receiver;
        let mut enumerated = false;
        loop {
            let inner = method(receiver)?;
            if bare(inner, "enumerate") || bare(inner, "rev") {
                enumerated |= inner.method == "enumerate";
                receiver = &inner.receiver;
            } else if bare(inner, "iter") {
                let guard = method(&inner.receiver)?;
                return (guard.method == "scan_for_v1").then_some((guard, enumerated));
            } else {
                return None;
            }
        }
    }

    fn loop_kinds(pat: &Pat, enumerated: bool, body: &syn::Block) -> Result<Kinds, String> {
        let event = bound_event(pat, enumerated).ok_or("loop binding")?;
        match body.stmts.as_slice() {
            [Stmt::Local(local), ..]
                if local.init.as_ref().is_some_and(|init| {
                    is_event(&init.expr, &event)
                        && init
                            .diverge
                            .as_ref()
                            .is_some_and(|(_, otherwise)| only_continues(otherwise))
                }) =>
            {
                variants(&local.pat)
            }
            [Stmt::Expr(Expr::Match(choice), _)] if is_event(&choice.expr, &event) => {
                arm_kinds(&choice.arms, does_nothing)
            }
            [] => Err("loop body".into()),
            statements => {
                let mut kinds = Kinds::new();
                for statement in statements {
                    let Stmt::Expr(Expr::If(test), _) = statement else {
                        return Err("loop body".into());
                    };
                    match &*test.cond {
                        Expr::Let(binding)
                            if test.else_branch.is_none() && is_event(&binding.expr, &event) =>
                        {
                            kinds.extend(variants(&binding.pat)?)
                        }
                        _ => return Err("loop body".into()),
                    }
                }
                Ok(kinds)
            }
        }
    }

    fn adapter_kinds(call: &ExprMethodCall, enumerated: bool) -> Result<Kinds, String> {
        let [Expr::Closure(closure)] = call.args.iter().collect::<Vec<_>>()[..] else {
            return Err("adapter argument".into());
        };
        let [param] = closure.inputs.iter().collect::<Vec<_>>()[..] else {
            return Err("closure parameters".into());
        };
        let event = bound_event(param, enumerated).ok_or("closure parameter")?;
        let optional = call.method == "filter_map" || call.method == "find_map";
        let nothing = if optional { "None" } else { "false" };
        match unblock(&closure.body) {
            Expr::Macro(invocation) if !optional => matched(invocation, &event),
            Expr::MethodCall(then)
                if optional
                    && (then.method == "then_some"
                        && then.args.len() == 1
                        && is_plain(&then.args[0])
                        || then.method == "then"
                            && matches!(then.args.first(), Some(Expr::Closure(_)))
                            && then.args.len() == 1) =>
            {
                match unblock(&then.receiver) {
                    Expr::Macro(invocation) => matched(invocation, &event),
                    _ => Err("closure body".into()),
                }
            }
            Expr::Match(choice) if is_event(&choice.expr, &event) => {
                arm_kinds(&choice.arms, |body| is_value(body, nothing))
            }
            Expr::If(test) => match (&*test.cond, &test.else_branch) {
                (Expr::Let(binding), Some((_, otherwise)))
                    if is_event(&binding.expr, &event) && is_value(otherwise, nothing) =>
                {
                    variants(&binding.pat)
                }
                _ => Err("closure body".into()),
            },
            Expr::Block(block) => match block.block.stmts.first() {
                Some(Stmt::Local(local))
                    if local.init.as_ref().is_some_and(|init| {
                        is_event(&init.expr, &event)
                            && init
                                .diverge
                                .as_ref()
                                .is_some_and(|(_, otherwise)| only_returns(otherwise, nothing))
                    }) =>
                {
                    variants(&local.pat)
                }
                _ => Err("closure body".into()),
            },
            _ => Err("closure body".into()),
        }
    }

    fn matched(invocation: &syn::ExprMacro, event: &str) -> Result<Kinds, String> {
        if !invocation.mac.path.is_ident("matches") {
            return Err("closure body".into());
        }
        let (scrutinee, pat) = invocation
            .mac
            .parse_body_with(parse_matches)
            .map_err(|error| format!("matches!: {error}"))?;
        if !is_event(&scrutinee, event) {
            return Err("matches! scrutinee".into());
        }
        variants(&pat)
    }

    /// A value that cannot fail or act: `then_some` evaluates it for events
    /// of every kind.
    fn is_plain(expr: &Expr) -> bool {
        match expr {
            Expr::Path(path) => path.qself.is_none(),
            Expr::Lit(_) => true,
            Expr::Cast(cast) => is_plain(&cast.expr),
            Expr::Paren(inner) => is_plain(&inner.expr),
            Expr::Reference(inner) => is_plain(&inner.expr),
            Expr::Tuple(tuple) => tuple.elems.iter().all(is_plain),
            _ => false,
        }
    }

    fn parse_matches(input: syn::parse::ParseStream) -> syn::Result<(Expr, Pat)> {
        let scrutinee: Expr = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let pat = Pat::parse_multi_with_leading_vert(input)?;
        if input.peek(syn::Token![if]) {
            input.parse::<syn::Token![if]>()?;
            input.parse::<Expr>()?;
        }
        if input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
        }
        Ok((scrutinee, pat))
    }

    fn arm_kinds(arms: &[syn::Arm], nothing: impl Fn(&Expr) -> bool) -> Result<Kinds, String> {
        let mut kinds = Kinds::new();
        for arm in arms {
            if matches!(arm.pat, Pat::Wild(_)) && arm.guard.is_none() {
                if !nothing(&arm.body) {
                    return Err("the `_` arm acts".into());
                }
            } else {
                kinds.extend(variants(&arm.pat)?);
            }
        }
        Ok(kinds)
    }

    /// The name one step of the scan binds to the event.
    fn bound_event(pat: &Pat, enumerated: bool) -> Option<String> {
        match pat {
            Pat::Reference(inner) => bound_event(&inner.pat, enumerated),
            Pat::Tuple(pair) if enumerated && pair.elems.len() == 2 => {
                bound_event(&pair.elems[1], false)
            }
            Pat::Ident(name) if !enumerated && name.subpat.is_none() => {
                Some(name.ident.to_string())
            }
            _ => None,
        }
    }

    fn is_event(expr: &Expr, event: &str) -> bool {
        match expr {
            Expr::Paren(inner) => is_event(&inner.expr, event),
            Expr::Reference(inner) => is_event(&inner.expr, event),
            Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Deref(_)) => {
                is_event(&unary.expr, event)
            }
            Expr::Path(path) => path.qself.is_none() && path.path.is_ident(event),
            _ => false,
        }
    }

    fn unblock(expr: &Expr) -> &Expr {
        match expr {
            Expr::Paren(inner) => unblock(&inner.expr),
            Expr::Block(block) => match block.block.stmts.as_slice() {
                [Stmt::Expr(inner, None)] => unblock(inner),
                _ => expr,
            },
            _ => expr,
        }
    }

    fn only_continues(expr: &Expr) -> bool {
        match expr {
            Expr::Block(block) => matches!(
                block.block.stmts.as_slice(),
                [Stmt::Expr(Expr::Continue(next), _)] if next.label.is_none()
            ),
            _ => false,
        }
    }

    fn only_returns(expr: &Expr, value: &str) -> bool {
        match expr {
            Expr::Block(block) => matches!(
                block.block.stmts.as_slice(),
                [Stmt::Expr(Expr::Return(answer), _)]
                    if answer.expr.as_ref().is_some_and(|answer| is_value(answer, value))
            ),
            _ => false,
        }
    }

    fn does_nothing(expr: &Expr) -> bool {
        match expr {
            Expr::Block(block) => block.block.stmts.is_empty() || only_continues(expr),
            Expr::Tuple(unit) => unit.elems.is_empty(),
            Expr::Continue(next) => next.label.is_none(),
            _ => false,
        }
    }

    fn is_value(expr: &Expr, value: &str) -> bool {
        match unblock(expr) {
            Expr::Path(path) => path.qself.is_none() && path.path.is_ident(value),
            Expr::Lit(literal) => {
                matches!(&literal.lit, syn::Lit::Bool(flag) if !flag.value && value == "false")
            }
            _ => false,
        }
    }

    /// The `CommittedEvent` variants a pattern can match; an error when it can
    /// match an event of any other kind.
    fn variants(pat: &Pat) -> Result<Kinds, String> {
        match pat {
            Pat::Or(alternatives) => {
                let mut kinds = Kinds::new();
                for case in &alternatives.cases {
                    kinds.extend(variants(case)?);
                }
                Ok(kinds)
            }
            Pat::Paren(inner) => variants(&inner.pat),
            Pat::Reference(inner) => variants(&inner.pat),
            Pat::Type(typed) => variants(&typed.pat),
            Pat::Ident(name) => match &name.subpat {
                Some((_, inner)) => variants(inner),
                None => Err(format!("binding pattern `{}`", name.ident)),
            },
            Pat::Struct(fields) => variant(&fields.path),
            Pat::TupleStruct(fields) => variant(&fields.path),
            Pat::Path(path) => variant(&path.path),
            _ => Err("pattern".into()),
        }
    }

    fn variant(path: &syn::Path) -> Result<Kinds, String> {
        let names: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        match names.as_slice() {
            [.., owner, kind] if owner == "CommittedEvent" || owner == "E" => {
                Ok(Kinds::from([kind.clone()]))
            }
            _ => Err(format!("pattern path `{}`", names.join("::"))),
        }
    }

    fn listed(guard: &ExprMethodCall, module: &[String], consts: &Lists) -> Result<Kinds, String> {
        let [list] = guard.args.iter().collect::<Vec<_>>()[..] else {
            return Err("guard arguments".into());
        };
        match list {
            Expr::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
                let name = path.path.segments[0].ident.to_string();
                consts
                    .get(&(module.to_vec(), name.clone()))
                    .cloned()
                    .ok_or_else(|| {
                        format!("the guard names `{name}`, not a kind list of its own module")
                    })
            }
            list => kind_list(list),
        }
    }

    fn kind_list(expr: &Expr) -> Result<Kinds, String> {
        let array = match expr {
            Expr::Reference(reference) => match &*reference.expr {
                Expr::Array(array) => array,
                _ => return Err("kind list".into()),
            },
            Expr::Array(array) => array,
            _ => return Err("kind list".into()),
        };
        let mut kinds = Kinds::new();
        for element in &array.elems {
            let Expr::Path(path) = element else {
                return Err("kind list element".into());
            };
            let names: Vec<String> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let [.., owner, kind] = names.as_slice() else {
                return Err("kind list element".into());
            };
            if owner != "EventKindV1" || !kinds.insert(kind.clone()) {
                return Err(format!("kind list element `{}`", names.join("::")));
            }
        }
        Ok(kinds)
    }

    fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).expect("source directory reads") {
            let path = entry.expect("source entry reads").path();
            if path.is_dir() {
                rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    #[test]
    fn every_history_guard_lists_exactly_the_kinds_its_scan_acts_on() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        let mut report = Report::default();
        for path in files {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if name.starts_with("event_history_v1") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("source file reads");
            if source.contains("scan_for_v1") {
                let file = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                audit_source(&file, &source, &mut report);
            }
        }
        assert!(
            report.failures.is_empty(),
            "{} history guards fail the audit:\n{}",
            report.failures.len(),
            report.failures.join("\n")
        );
        assert!(
            report.guards > 0,
            "no history guard found under {}",
            root.display()
        );
        eprintln!("{} history guards audited", report.guards);
    }

    /// The audit reads kinds by name, so every `CommittedEvent` variant must
    /// count as the `EventKindV1` value of its own name.
    #[test]
    fn each_event_counts_as_the_kind_of_its_name() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let parse = |name: &str| {
            let source = std::fs::read_to_string(root.join(name)).expect("source file reads");
            syn::parse_file(&source).expect("source file parses")
        };
        let enum_variants = |file: &syn::File, name: &str| -> Vec<String> {
            file.items
                .iter()
                .find_map(|item| match item {
                    syn::Item::Enum(list) if list.ident == name => {
                        Some(list.variants.iter().map(|v| v.ident.to_string()).collect())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no enum {name}"))
        };
        let events = enum_variants(&parse("event.rs"), "CommittedEvent");
        let index = parse("event_history_v1.rs");
        assert_eq!(enum_variants(&index, "EventKindV1"), events);
        let kind_of = index
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Impl(block) => block.items.iter().find_map(|item| match item {
                    syn::ImplItem::Fn(function) if function.sig.ident == "kind_v1" => {
                        Some(function)
                    }
                    _ => None,
                }),
                _ => None,
            })
            .expect("kind_v1 is defined");
        let [Stmt::Expr(Expr::Match(choice), None)] = kind_of.block.stmts.as_slice() else {
            panic!("kind_v1 is one match");
        };
        assert_eq!(choice.arms.len(), events.len());
        for arm in &choice.arms {
            let from = match &arm.pat {
                Pat::Struct(fields) => fields.path.segments.last(),
                Pat::TupleStruct(fields) => fields.path.segments.last(),
                Pat::Path(path) => path.path.segments.last(),
                _ => None,
            }
            .filter(|_| arm.guard.is_none())
            .expect("each arm names one variant");
            let Expr::Path(to) = unblock(&arm.body) else {
                panic!("each arm gives one kind");
            };
            assert_eq!(from.ident, to.path.segments.last().expect("kind").ident);
        }
    }

    fn audit(source: &str) -> Report {
        let mut report = Report::default();
        audit_source("snippet.rs", source, &mut report);
        report
    }

    #[test]
    fn the_audit_reads_every_accepted_shape() {
        let report = audit(
            r#"
            const TAPS: &[EventKindV1] = &[EventKindV1::Tap];
            fn shapes(h: &EventHistoryV1) {
                for e in h.scan_for_v1(&[EventKindV1::Tap]) {
                    let CommittedEvent::Tap { object } = e else { continue; };
                    note(object);
                }
                for (at, e) in h.scan_for_v1(&[EventKindV1::Tap, EventKindV1::Draw]).iter().enumerate() {
                    match e {
                        CommittedEvent::Tap { .. } | CommittedEvent::Draw { .. } => note(at),
                        _ => {}
                    }
                }
                for e in h.scan_for_v1(&[EventKindV1::Draw]).iter() {
                    if let CommittedEvent::Draw { player } = e { note(player); }
                }
                let _ = h.scan_for_v1(TAPS).iter().rev().any(|e| matches!(e, CommittedEvent::Tap { object } if object.0 == 1));
                let _ = h.scan_for_v1(&[EventKindV1::Draw]).iter().enumerate().filter_map(|(at, e)| match e {
                    E::Draw { player } => Some((at, player)),
                    _ => None,
                });
                let _ = h.scan_for_v1(&[EventKindV1::Tap]).iter().position(|e| match e {
                    CommittedEvent::Tap { object } => object.0 == 2,
                    _ => false,
                });
                for (at, e) in h.scan_for_v1(&[EventKindV1::Tap, EventKindV1::Draw]).iter().enumerate() {
                    if let CommittedEvent::Tap { object } = e { note((at, object)); }
                    if let CommittedEvent::Draw { player } = e { note(player); }
                }
                let _ = h.scan_for_v1(&[EventKindV1::Draw]).iter().enumerate().filter_map(|(at, e)| {
                    matches!(e, CommittedEvent::Draw { .. }).then_some(at as u32)
                });
                let _ = h.scan_for_v1(&[EventKindV1::Tap]).iter().filter_map(|e| {
                    if let CommittedEvent::Tap { object } = e { Some(object) } else { None }
                });
                let _ = h.scan_for_v1(&[EventKindV1::Tap]).iter().any(|e| {
                    let CommittedEvent::Tap { object } = e else { return false; };
                    object.0 == 3
                });
            }
            "#,
        );
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.guards, 10);
    }

    #[test]
    fn the_audit_refuses_a_guard_that_differs_from_its_scan() {
        for source in [
            // A kind the scan acts on is missing from the list.
            "fn f(h: &H) { let _ = h.scan_for_v1(&[EventKindV1::Tap]).iter().any(|e| matches!(e, CommittedEvent::Tap { .. } | CommittedEvent::Draw { .. })); }",
            // A listed kind the scan never acts on.
            "fn f(h: &H) { let _ = h.scan_for_v1(&[EventKindV1::Tap, EventKindV1::Draw]).iter().any(|e| matches!(e, CommittedEvent::Tap { .. })); }",
            // The loop acts on every event before its pattern.
            "fn f(h: &H) { for e in h.scan_for_v1(&[EventKindV1::Tap]) { count(); if let CommittedEvent::Tap { .. } = e { note(); } } }",
            // The `_` arm acts.
            "fn f(h: &H) { for e in h.scan_for_v1(&[EventKindV1::Tap]) { match e { CommittedEvent::Tap { .. } => note(), _ => count(), } } }",
            // An event of another kind ends the scan.
            "fn f(h: &H) -> bool { for e in h.scan_for_v1(&[EventKindV1::Tap]) { let CommittedEvent::Tap { .. } = e else { return false; }; } true }",
            // A binding arm matches every kind.
            "fn f(h: &H) { let _ = h.scan_for_v1(&[EventKindV1::Tap]).iter().filter_map(|e| match e { CommittedEvent::Tap { .. } => Some(1), other => Some(2), }); }",
            // `all` holds over a skipped scan.
            "fn f(h: &H) -> bool { h.scan_for_v1(&[EventKindV1::Tap]).iter().all(|e| matches!(e, CommittedEvent::Tap { .. })) }",
            // `then_some` computes its value for events of every kind.
            "fn f(h: &H) { let _ = h.scan_for_v1(&[EventKindV1::Tap]).iter().filter_map(|e| matches!(e, CommittedEvent::Tap { .. }).then_some(cost(e))); }",
            // An `if let` with an `else` that acts.
            "fn f(h: &H) { for e in h.scan_for_v1(&[EventKindV1::Tap]) { if let CommittedEvent::Tap { .. } = e { note(); } else { count(); } } }",
            // The guard's length is not a scan.
            "fn f(h: &H) -> usize { h.scan_for_v1(&[EventKindV1::Tap]).len() }",
            // A guard inside a macro call is not parsed.
            "fn f(h: &H) { assert!(h.scan_for_v1(&[EventKindV1::Tap]).is_empty()); }",
            // A list the file does not define.
            "fn f(h: &H) -> bool { h.scan_for_v1(OTHER).iter().any(|e| matches!(e, CommittedEvent::Tap { .. })) }",
            // A list another module defines.
            "mod a { const K: &[EventKindV1] = &[EventKindV1::Tap]; } fn f(h: &H) -> bool { h.scan_for_v1(K).iter().any(|e| matches!(e, CommittedEvent::Tap { .. })) }",
            // A name the file defines twice: the guard may mean either list.
            "const K: &[EventKindV1] = &[EventKindV1::Tap]; fn f(h: &H) -> bool { const K: &[EventKindV1] = &[EventKindV1::Draw]; h.scan_for_v1(K).iter().any(|e| matches!(e, CommittedEvent::Tap { .. })) }",
        ] {
            assert_eq!(audit(source).failures.len(), 1, "{source}");
        }
    }
}

#[test]
fn a_prefix_copy_resumes_its_chain_and_digests_like_a_plain_copy() {
    let mut history = EventHistoryV1::new();
    for id in 0..60 {
        history.push(tap(id));
        if id % 3 == 0 {
            history.chain_digest_v2();
        }
    }
    history.chain_digest_v2();
    assert_eq!(history.chain.get().len(), CHAIN_V2_PREFIXES);
    for len in [0, 1, 4, 10, 37, 59, 60, 61] {
        let prefix = history.prefix_v1(len);
        let plain = EventHistoryV1::from(history.events[..len.min(60)].to_vec());
        assert_eq!(*prefix, *plain);
        assert!(counted(&prefix));
        assert!(prefix
            .chain
            .get()
            .iter()
            .all(|(covered, _)| *covered <= len));
        assert_eq!(prefix.chain_digest_v2(), plain.chain_digest_v2());
    }
    // A kept prefix state is resumed, not recomputed from the first event.
    let prefix = history.prefix_v1(37);
    assert_eq!(
        prefix.chain.longest(37).map(|(covered, _)| covered),
        Some(37)
    );
}

#[test]
fn a_long_digest_keeps_stride_prefixes_for_any_earlier_copy() {
    let mut history = EventHistoryV1::new();
    for id in 0..300 {
        history.push(tap(id));
    }
    history.chain_digest_v2();
    // Recent digests past the stride never evict a stride prefix.
    for id in 300..340 {
        history.push(tap(id));
        history.chain_digest_v2();
    }
    for len in [0, 63, 64, 65, 150, 299, 301] {
        let prefix = history.prefix_v1(len);
        let covered = prefix.chain.longest(len).map_or(0, |(covered, _)| covered);
        assert!(
            len - covered < CHAIN_V2_STRIDE,
            "{len} resumes at {covered}"
        );
        let plain = EventHistoryV1::from(history.events[..len].to_vec());
        assert_eq!(prefix.chain_digest_v2(), plain.chain_digest_v2());
    }
}

#[test]
fn a_prefix_memo_lasts_exactly_while_its_events_are_unchanged() {
    let memo = |history: &EventHistoryV1| history.prefix_memo_v1().map(|(covered, _)| covered);
    let mut history = EventHistoryV1::from((0..10).map(tap).collect::<Vec<_>>());
    assert_eq!(memo(&history), None);
    history.set_prefix_memo_v1(11, Arc::new(()));
    assert_eq!(memo(&history), None, "a memo past the end is never left");
    history.set_prefix_memo_v1(6, Arc::new(()));
    assert_eq!(memo(&history), Some(6));

    // Appending and changing only later events keep it; clones carry it.
    history.push(tap(10));
    history.insert(6, tap(11));
    history.remove(6);
    history.truncate(8);
    history.pop();
    assert_eq!(memo(&history), Some(6));
    assert_eq!(memo(&history.clone()), Some(6));
    assert_eq!(memo(&history.prefix_v1(6)), Some(6));
    assert_eq!(memo(&history.prefix_v1(5)), None);

    // Never part of what the history is.
    let plain = EventHistoryV1::from(history.events.clone());
    assert_eq!(history, plain);
    assert_eq!(
        serde_json::to_vec(&history).unwrap(),
        serde_json::to_vec(&plain).unwrap()
    );
    let restored: EventHistoryV1 =
        serde_json::from_slice(&serde_json::to_vec(&history).unwrap()).unwrap();
    assert_eq!(memo(&restored), None);

    // Any change to the covered events drops it.
    type Change = fn(&mut EventHistoryV1);
    let changes: [Change; 7] = [
        |h| h.insert(5, tap(20)),
        |h| {
            h.remove(5);
        },
        |h| h.truncate(5),
        |h| h.retain(|_| true),
        |h| h.clear(),
        |h| h[0] = tap(21),
        |h| {
            for _ in h.iter_mut() {}
        },
    ];
    for change in changes {
        let mut changed = history.clone();
        change(&mut changed);
        assert_eq!(memo(&changed), None);
        assert_eq!(memo(&history), Some(6));
    }
    let mut short = history.clone();
    short.truncate(6);
    assert_eq!(memo(&short), Some(6));
    short.pop();
    assert_eq!(memo(&short), None);
}
