//! Module system: `ModuleExprVisitor`, `ModuleSystemTypeChecker` and
//! `ModuleSystemDeclarations` upstream, plus parametric-test expansion
//! (`ParamAssignment`, `ParamExprVisitor`, `SimpleExprEval`).

use crate::sema::analyzer::{Analyzer, FunSrc};
use crate::sema::check::Ctx;
use crate::sema::collect::parse_int_literal;
use crate::sema::diag::Fatal;
use crate::sema::program::*;
use crate::sema::types::PType;
use crate::syntax::ast;
use crate::syntax::Span;

/// A resolved module expression (names looked up, structure kept).
#[derive(Debug, Clone)]
pub enum RMod {
    Bind(Vec<(InterfaceId, MachineId)>, Span),
    /// `is_composition`: `compose` (true) or `union` (false).
    Combine(Vec<RMod>, bool, Span),
    HideEvents(Vec<EventId>, Box<RMod>, Span),
    HideInterfaces(Vec<InterfaceId>, Box<RMod>, Span),
    Assert(Vec<MachineId>, Box<RMod>, Span),
    Rename {
        new: InterfaceId,
        old: InterfaceId,
        module: Box<RMod>,
        span: Span,
    },
}

impl RMod {
    pub fn span(&self) -> Span {
        match self {
            RMod::Bind(_, s)
            | RMod::Combine(_, _, s)
            | RMod::HideEvents(_, _, s)
            | RMod::HideInterfaces(_, _, s)
            | RMod::Assert(_, _, s) => *s,
            RMod::Rename { span, .. } => *span,
        }
    }
}

/// A parsed `param (x in [..], ..)` clause: parameter name, its global variable, the values.
struct ParamSpec {
    var: VarId,
    values: Vec<ParamValue>,
}

fn sorted_dedup_by_name<'n, T: Copy + PartialEq>(mut v: Vec<T>, name: impl Fn(T) -> &'n str) -> Vec<T> {
    v.sort_by(|a, b| name(*a).cmp(name(*b)));
    v.dedup();
    v
}

impl<'a> Analyzer<'a> {
    fn event_name(&self, e: EventId) -> String {
        self.prog.events[e.idx()].name.clone()
    }
    fn iface_name(&self, i: InterfaceId) -> String {
        self.prog.interfaces[i.idx()].name.clone()
    }
    fn sorted_events(&self, v: Vec<EventId>) -> Vec<EventId> {
        sorted_dedup_by_name(v, |e| &self.prog.events[e.idx()].name)
    }
    fn sorted_ifaces(&self, v: Vec<InterfaceId>) -> Vec<InterfaceId> {
        sorted_dedup_by_name(v, |i| &self.prog.interfaces[i.idx()].name)
    }
    /// `GetPermissions`: permissions embedded in a type, or the universal event set when the type
    /// admits any permission.
    fn permissions_of(&self, t: &PType) -> Vec<EventId> {
        match t.allowed_permissions(&self.prog) {
            None => self.prog.universal_events(),
            Some(v) => v,
        }
    }

    // ------------------------------------------------------------------
    // ModuleExprVisitor
    // ------------------------------------------------------------------

    pub fn resolve_mod_expr(&mut self, e: &'a ast::ModExpr, depth: usize) -> Result<RMod, Fatal> {
        let g = self.scopes.global();
        if depth > 256 {
            return Err(self
                .e
                .internal_error(e.span, "module expression nesting is too deep")
                .help("name parts of the module with `module M = ...;` and refer to them by name"));
        }
        Ok(match &e.kind {
            ast::ModExprKind::Paren(inner) | ast::ModExprKind::Main { module: inner, .. } => {
                self.resolve_mod_expr(inner, depth + 1)?
            }
            ast::ModExprKind::Named(name) => {
                let Some(idx) = self.scopes.lookup_named_module(g, &name.name) else {
                    return Err(self.e.missing_declaration(name.span, "module", &name.name));
                };
                let (_, expr, _) = self.module_src[idx as usize];
                self.resolve_mod_expr(expr, depth + 1)?
            }
            ast::ModExprKind::Bind(binds) => {
                let mut out = Vec::new();
                for b in binds {
                    let iname = b.interface.as_ref().unwrap_or(&b.machine);
                    let Some(i) = self.scopes.lookup_interface(g, &iname.name) else {
                        let d = self.e.missing_declaration(iname.span, "interface", &iname.name);
                        let spec = self
                            .scopes
                            .lookup_machine(g, &iname.name)
                            .filter(|m| self.prog.machines[m.idx()].is_spec());
                        return Err(match spec {
                            Some(m) if self.prog.machines[m.idx()].is_scenario() => d.help(format!(
                                "'{}' is a scenario: it watches every test case on its own, so remove it from the machine list",
                                iname.name
                            )),
                            Some(_) => d.help(format!(
                                "'{0}' is a spec, not a machine: remove it from the machine list and attach it with `assert`, \
                                 e.g. `test tc [main=Main]: assert {0} in {{ Main, ... }};`",
                                iname.name
                            )),
                            None => d,
                        });
                    };
                    let Some(m) = self.scopes.lookup_machine(g, &b.machine.name) else {
                        return Err(self.e.missing_declaration(b.machine.span, "machine", &b.machine.name));
                    };
                    out.push((i, m));
                }
                let mut seen: Vec<InterfaceId> = Vec::new();
                for (i, _) in &out {
                    if seen.contains(i) {
                        return Err(self.e.duplicate_bindings(e.span, &self.iface_name(*i)));
                    }
                    seen.push(*i);
                }
                RMod::Bind(out, e.span)
            }
            ast::ModExprKind::Compose(list) | ast::ModExprKind::Union(list) => {
                let mut mods = Vec::new();
                for m in list {
                    mods.push(self.resolve_mod_expr(m, depth + 1)?);
                }
                RMod::Combine(mods, matches!(e.kind, ast::ModExprKind::Compose(_)), e.span)
            }
            ast::ModExprKind::HideEvents { events, module } => {
                let mut evs = Vec::new();
                for n in events {
                    let Some(ev) = self.scopes.lookup_event(g, &n.name) else {
                        return Err(self.e.missing_declaration(n.span, "event", &n.name));
                    };
                    evs.push(ev);
                }
                let evs = self.sorted_events(evs);
                RMod::HideEvents(evs, Box::new(self.resolve_mod_expr(module, depth + 1)?), e.span)
            }
            ast::ModExprKind::HideInterfaces { interfaces, module } => {
                let mut ifs = Vec::new();
                for n in interfaces {
                    let Some(i) = self.scopes.lookup_interface(g, &n.name) else {
                        return Err(self.e.missing_declaration(n.span, "interface", &n.name));
                    };
                    ifs.push(i);
                }
                RMod::HideInterfaces(ifs, Box::new(self.resolve_mod_expr(module, depth + 1)?), e.span)
            }
            ast::ModExprKind::Rename { old, new, module } => {
                let Some(ni) = self.scopes.lookup_interface(g, &new.name) else {
                    return Err(self.e.missing_declaration(new.span, "interface", &new.name));
                };
                let Some(oi) = self.scopes.lookup_interface(g, &old.name) else {
                    return Err(self.e.missing_declaration(old.span, "interface", &old.name));
                };
                RMod::Rename {
                    new: ni,
                    old: oi,
                    module: Box::new(self.resolve_mod_expr(module, depth + 1)?),
                    span: e.span,
                }
            }
            ast::ModExprKind::Assert { monitors, module } => {
                let mut mons = Vec::new();
                for n in monitors {
                    let Some(m) = self.scopes.lookup_machine(g, &n.name) else {
                        return Err(self.e.missing_declaration(n.span, "spec monitor", &n.name));
                    };
                    if !self.prog.machines[m.idx()].is_spec() {
                        return Err(self.e.expected_monitor(n.span, &n.name));
                    }
                    mons.push(m);
                }
                RMod::Assert(mons, Box::new(self.resolve_mod_expr(module, depth + 1)?), e.span)
            }
        })
    }

    // ------------------------------------------------------------------
    // ModuleSystemTypeChecker.CheckWellFormedness
    // ------------------------------------------------------------------

    pub fn module_info(&mut self, m: &RMod) -> Result<ModuleInfo, Fatal> {
        match m {
            RMod::Bind(bindings, span) => self.info_bind(bindings, *span),
            RMod::Combine(mods, is_comp, _) => self.info_combine(mods, *is_comp),
            RMod::HideEvents(evs, inner, span) => self.info_hide_events(evs, inner, *span),
            RMod::HideInterfaces(ifs, inner, span) => self.info_hide_interfaces(ifs, inner, *span),
            RMod::Assert(mons, inner, span) => self.info_assert(mons, inner, *span),
            RMod::Rename { new, old, module, span } => self.info_rename(*new, *old, module, *span),
        }
    }

    fn creates_via_links(&self, info: &mut ModuleInfo) {
        let mut creates = Vec::new();
        for (iface, machine) in info.interface_def.clone() {
            for created in &self.prog.machines[machine.idx()].creates {
                if let Some(target) = info.link(iface, *created) {
                    creates.push(target);
                }
            }
        }
        info.creates = self.sorted_ifaces(creates);
    }

    fn info_bind(&mut self, bindings: &[(InterfaceId, MachineId)], span: Span) -> Result<ModuleInfo, Fatal> {
        for (i, m) in bindings {
            let ireceives = self.prog.interface_receives(*i);
            let mreceives = self.prog.machine_receives(*m);
            if !ireceives.iter().all(|e| mreceives.contains(e)) {
                let msg = format!(
                    "receive set of {} is not a subset of receive set of {}",
                    self.iface_name(*i),
                    self.prog.machines[m.idx()].name
                );
                return Err(self.e.invalid_bind(span, &msg));
            }
            let ipayload = self.prog.interfaces[i.idx()].payload.clone();
            let mpayload = self.prog.machines[m.idx()].payload.clone();
            if !mpayload.is_assignable_from(&ipayload, &self.prog) {
                let msg = format!(
                    "payload type of {} is not a subtype of payload type of {}",
                    self.iface_name(*i),
                    self.prog.machines[m.idx()].name
                );
                return Err(self.e.invalid_bind(span, &msg));
            }
        }
        let mut info = ModuleInfo::default();
        for (i, m) in bindings {
            info.interface_def.push((*i, *m));
        }
        for (i, m) in bindings {
            let links: Vec<(InterfaceId, InterfaceId)> =
                self.prog.machines[m.idx()].creates.iter().map(|c| (*c, *c)).collect();
            info.link_map.push((*i, links));
        }
        let mut sends = Vec::new();
        let mut receives = Vec::new();
        for (_, m) in bindings {
            sends.extend(self.prog.machine_sends(*m));
            receives.extend(self.prog.machine_receives(*m));
        }
        info.sends = self.sorted_events(sends);
        info.receives = self.sorted_events(receives);
        self.creates_via_links(&mut info);
        Ok(info)
    }

    fn info_assert(&mut self, monitors: &[MachineId], inner: &RMod, span: Span) -> Result<ModuleInfo, Fatal> {
        let comp = self.module_info(inner)?;
        for mon in monitors {
            let observes = self.prog.machines[mon.idx()].observes.clone();
            if let Some(ev) = observes.iter().find(|e| !comp.sends.contains(e)) {
                let mname = self.prog.machines[mon.idx()].name.clone();
                return Err(self.e.invalid_assert_event(span, &mname, &self.event_name(*ev)));
            }
        }
        for (existing, _) in &comp.monitor_map {
            if monitors.contains(existing) {
                let mname = self.prog.machines[existing.idx()].name.clone();
                return Err(self.e.invalid_assert_monitor(span, &mname));
            }
        }
        let mut info = comp.clone();
        for mon in monitors {
            let all: Vec<InterfaceId> = comp.interface_def.iter().map(|(i, _)| *i).collect();
            info.monitor_map.push((*mon, all));
        }
        Ok(info)
    }

    fn info_rename(
        &mut self,
        new: InterfaceId,
        old: InterfaceId,
        inner: &RMod,
        span: Span,
    ) -> Result<ModuleInfo, Fatal> {
        let comp = self.module_info(inner)?;
        let nr = self.prog.interface_receives(new);
        let or = self.prog.interface_receives(old);
        if nr != or {
            let msg = format!("{} and {} must have the same receive set", self.iface_name(new), self.iface_name(old));
            return Err(self.e.invalid_rename(span, &msg));
        }
        let bound_or_created =
            |i: InterfaceId| comp.creates.contains(&i) || comp.interface_def.iter().any(|(k, _)| *k == i);
        if !bound_or_created(old) {
            let msg = format!(
                "{} must belong to either created interfaces or bounded interfaces of the module",
                self.iface_name(old)
            );
            return Err(self.e.invalid_rename(span, &msg));
        }
        if bound_or_created(new) {
            let msg = format!(
                "{} must not belong to created interfaces or bounded interfaces of the module",
                self.iface_name(new)
            );
            return Err(self.e.invalid_rename(span, &msg));
        }
        let rn = |i: InterfaceId| if i == old { new } else { i };
        let mut info = ModuleInfo::default();
        for (m, ifs) in &comp.monitor_map {
            info.monitor_map.push((*m, ifs.iter().map(|i| rn(*i)).collect()));
        }
        info.private_interfaces = self.sorted_ifaces(comp.private_interfaces.iter().map(|i| rn(*i)).collect());
        for (i, m) in &comp.interface_def {
            info.interface_def.push((rn(*i), *m));
        }
        for (k, links) in &comp.link_map {
            info.link_map.push((rn(*k), links.iter().map(|(a, b)| (*a, rn(*b))).collect()));
        }
        info.sends = comp.sends.clone();
        info.receives = comp.receives.clone();
        info.private_events = comp.private_events.clone();
        self.creates_via_links(&mut info);
        Ok(info)
    }

    fn info_combine(&mut self, mods: &[RMod], is_composition: bool) -> Result<ModuleInfo, Fatal> {
        let mut infos = Vec::new();
        for m in mods {
            infos.push(self.module_info(m)?);
        }
        for (a, m1) in infos.iter().enumerate() {
            for (b, m2) in infos.iter().enumerate() {
                if a == b {
                    continue;
                }
                let loc1 = mods[a].span();
                let loc2 = mods[b].span();
                let mut all_private: Vec<EventId> = m1.private_events.clone();
                for e in &m2.private_events {
                    if !all_private.contains(e) {
                        all_private.push(*e);
                    }
                }
                let mut all_sr: Vec<EventId> = Vec::new();
                for e in m1.sends.iter().chain(&m1.receives).chain(&m2.receives).chain(&m2.sends) {
                    if !all_sr.contains(e) {
                        all_sr.push(*e);
                    }
                }
                // 1) bound interfaces are disjoint
                for (i, _) in &m1.interface_def {
                    if m2.interface_def.iter().any(|(j, _)| j == i) {
                        let msg = format!(
                            "bound interfaces after composition are not disjoint, e.g., interface {} is bound in both the modules being composed",
                            self.iface_name(*i)
                        );
                        return Err(self.e.invalid_composition(loc1, &msg));
                    }
                }
                // 2) no private events in sends/receives
                for e in &all_sr {
                    if all_private.contains(e) {
                        let msg = format!(
                            "private events after composition are not disjoint from send and receives set, e.g., after composition private event {} belongs to both private and public (sends or receives) events",
                            self.event_name(*e)
                        );
                        return Err(self.e.invalid_composition(loc1, &msg));
                    }
                }
                // 3) no private events in permissions of sends/receives
                for e in &all_sr {
                    let payload = self.prog.events[e.idx()].payload.clone();
                    let perms = self.permissions_of(&payload);
                    if let Some(p) = all_private.iter().find(|p| perms.contains(p)) {
                        let msg = format!(
                            "private events after composition are not disjoint from permissions in events sent or received, e.g., after composition private event {} is in the permissions set of {}",
                            self.event_name(*p),
                            self.event_name(*e)
                        );
                        return Err(self.e.invalid_composition(loc1, &msg));
                    }
                }
                let keys = |m: &ModuleInfo| -> Vec<InterfaceId> { m.interface_def.iter().map(|(i, _)| *i).collect() };
                let mut candidates: Vec<InterfaceId> = Vec::new();
                for m in [m1, m2] {
                    let ks = keys(m);
                    for i in m.creates.iter().filter(|i| !ks.contains(i)) {
                        if !candidates.contains(i) {
                            candidates.push(*i);
                        }
                    }
                    for i in ks.iter().filter(|i| !m.creates.contains(i)) {
                        if !candidates.contains(i) {
                            candidates.push(*i);
                        }
                    }
                }
                for i in &candidates {
                    let recv = self.prog.interface_receives(*i);
                    if let Some(e) = all_private.iter().find(|e| recv.contains(e)) {
                        let msg = format!(
                            "After composition, private event {} is in the received events of interface {} which is created or bound in the module",
                            self.event_name(*e),
                            self.iface_name(*i)
                        );
                        return Err(self.e.invalid_composition(loc1, &msg));
                    }
                }
                for (mon, _) in &m1.monitor_map {
                    if m2.monitor_map.iter().any(|(k, _)| k == mon) {
                        let msg = format!(
                            "monitor {} is attached in more than one modules being composed",
                            self.prog.machines[mon.idx()].name
                        );
                        return Err(self.e.invalid_composition(loc1, &msg));
                    }
                }
                if is_composition {
                    if let Some(e) = m1.sends.iter().find(|e| m2.sends.contains(e)) {
                        let msg = format!(
                            "output sends are not disjoint, {} belongs to the sends of multiple composed module",
                            self.event_name(*e)
                        );
                        return Err(self.e.invalid_composition(loc1, &msg));
                    }
                    if let Some(i) = m1.creates.iter().find(|i| m2.creates.contains(i)) {
                        let msg = format!(
                            "output creates are not disjoint, {} belongs to the creates of multiple composed module",
                            self.iface_name(*i)
                        );
                        return Err(self.e.invalid_composition(loc1, &msg));
                    }
                }
                let mut exported: Vec<InterfaceId> = keys(m1);
                for i in &m1.creates {
                    if !exported.contains(i) {
                        exported.push(*i);
                    }
                }
                for i in &exported {
                    let payload = self.prog.interfaces[i.idx()].payload.clone();
                    let perms = self.permissions_of(&payload);
                    if let Some(e) = m2.private_events.iter().find(|e| perms.contains(e)) {
                        let msg = format!(
                            "private event {} belongs to the permissions of the constructor type of public interface {}",
                            self.event_name(*e),
                            self.iface_name(*i)
                        );
                        return Err(self.e.invalid_hide_event(loc2, &msg));
                    }
                }
            }
        }
        let mut info = ModuleInfo::default();
        let mut private_events = Vec::new();
        let mut private_ifaces = Vec::new();
        let mut sends = Vec::new();
        let mut receives = Vec::new();
        let mut creates = Vec::new();
        for m in &infos {
            private_events.extend(m.private_events.iter().copied());
            private_ifaces.extend(m.private_interfaces.iter().copied());
            for (k, v) in &m.monitor_map {
                if let Some(slot) = info.monitor_map.iter_mut().find(|(kk, _)| kk == k) {
                    slot.1 = v.clone();
                } else {
                    info.monitor_map.push((*k, v.clone()));
                }
            }
            for (k, v) in &m.link_map {
                if let Some(slot) = info.link_map.iter_mut().find(|(kk, _)| kk == k) {
                    slot.1 = v.clone();
                } else {
                    info.link_map.push((*k, v.clone()));
                }
            }
            for (i, mach) in &m.interface_def {
                info.interface_def.push((*i, *mach));
            }
            sends.extend(m.sends.iter().copied());
            receives.extend(m.receives.iter().copied());
            creates.extend(m.creates.iter().copied());
        }
        info.private_events = self.sorted_events(private_events);
        info.private_interfaces = self.sorted_ifaces(private_ifaces);
        info.sends = self.sorted_events(sends);
        info.receives = self.sorted_events(receives);
        info.creates = self.sorted_ifaces(creates);
        Ok(info)
    }

    fn info_hide_events(&mut self, hide: &[EventId], inner: &RMod, span: Span) -> Result<ModuleInfo, Fatal> {
        let comp = self.module_info(inner)?;
        // 1) hidden events must be both sent and received
        let both: Vec<EventId> = comp.sends.iter().copied().filter(|e| comp.receives.contains(e)).collect();
        if let Some(e) = hide.iter().find(|h| !both.contains(h)) {
            let msg = format!(
                "event {} cannot be made private, it must belong to both receive and send set of the module",
                self.event_name(*e)
            );
            return Err(self.e.invalid_hide_event(span, &msg));
        }
        // 2) only events of interfaces both created and bound can be hidden
        let keys: Vec<InterfaceId> = comp.interface_def.iter().map(|(i, _)| *i).collect();
        let mut candidates: Vec<InterfaceId> = keys.iter().copied().filter(|i| !comp.creates.contains(i)).collect();
        for i in comp.creates.iter().filter(|i| !keys.contains(i)) {
            if !candidates.contains(i) {
                candidates.push(*i);
            }
        }
        for i in &candidates {
            let recv = self.prog.interface_receives(*i);
            if let Some(e) = hide.iter().find(|e| recv.contains(e)) {
                let msg = format!(
                    "event {} cannot be made private as interface {} contains this event. Only events in interfaces that are both created and bound in the module can be hidden",
                    self.event_name(*e),
                    self.iface_name(*i)
                );
                return Err(self.e.invalid_hide_event(span, &msg));
            }
        }
        // 3) remaining public events must not carry private permissions
        let mut public: Vec<EventId> = comp.sends.clone();
        for e in &comp.receives {
            if !public.contains(e) {
                public.push(*e);
            }
        }
        for e in public.iter().filter(|e| !hide.contains(e)) {
            let payload = self.prog.events[e.idx()].payload.clone();
            let perms = self.permissions_of(&payload);
            if let Some(p) = hide.iter().find(|p| perms.contains(p)) {
                let msg = format!(
                    "event {} cannot be made private as it belongs to allowed permission of {} which is received or sent by the module",
                    self.event_name(*p),
                    self.event_name(*e)
                );
                return Err(self.e.invalid_hide_event(span, &msg));
            }
        }
        // 4) (upstream dereferences the not-yet-computed module info here; the check it intends
        // is against the component's exported and created interfaces)
        let mut exported: Vec<InterfaceId> = keys.clone();
        for i in &comp.creates {
            if !exported.contains(i) {
                exported.push(*i);
            }
        }
        for i in &exported {
            let payload = self.prog.interfaces[i.idx()].payload.clone();
            let perms = self.permissions_of(&payload);
            if let Some(e) = hide.iter().find(|e| perms.contains(e)) {
                let msg = format!(
                    "event {} cannot be made private as it belongs to the permissions of the contructor type of interface {}",
                    self.event_name(*e),
                    self.iface_name(*i)
                );
                return Err(self.e.invalid_hide_event(span, &msg));
            }
        }
        let mut info = comp.clone();
        let mut pe = comp.private_events.clone();
        pe.extend(hide.iter().copied());
        info.private_events = self.sorted_events(pe);
        info.sends = comp.sends.iter().copied().filter(|e| !hide.contains(e)).collect();
        info.receives = comp.receives.iter().copied().filter(|e| !hide.contains(e)).collect();
        Ok(info)
    }

    fn info_hide_interfaces(&mut self, hide: &[InterfaceId], inner: &RMod, span: Span) -> Result<ModuleInfo, Fatal> {
        let comp = self.module_info(inner)?;
        for i in hide {
            let bound = comp.interface_def.iter().any(|(k, _)| k == i);
            if !(comp.creates.contains(i) && bound) {
                let n = self.iface_name(*i);
                let msg = format!(
                    "interface {} cannot be made private. Interface {} must be both created and bounded in the module",
                    n, n
                );
                return Err(self.e.invalid_hide_interface(span, &msg));
            }
        }
        let mut info = comp.clone();
        let mut pi = comp.private_interfaces.clone();
        pi.extend(hide.iter().copied());
        info.private_interfaces = self.sorted_ifaces(pi);
        Ok(info)
    }

    // ------------------------------------------------------------------
    // Test / implementation checks
    // ------------------------------------------------------------------

    fn not_closed_interface(info: &ModuleInfo) -> Option<InterfaceId> {
        info.creates.iter().copied().find(|i| !info.interface_def.iter().any(|(k, _)| k == i))
    }

    fn check_safety_test(&mut self, info: &ModuleInfo, main: &ast::Ident, span: Span) -> Result<MachineId, Fatal> {
        if let Some(i) = Self::not_closed_interface(info) {
            let msg = format!(
                "test module is not closed with respect to created interfaces; interface {} is created but not implemented inside the module",
                self.iface_name(i)
            );
            return Err(self.e.not_closed(span, &msg));
        }
        let has_main = info
            .interface_def
            .iter()
            .any(|(_, m)| self.prog.machines[m.idx()].name == main.name && !self.prog.machines[m.idx()].is_spec());
        if !has_main {
            let msg = format!("machine {} does not exist in the test module", main.name);
            return Err(self.e.no_main(span, &msg));
        }
        let g = self.scopes.global();
        let mid = self.scopes.lookup_machine(g, &main.name).expect("main machine exists");
        if !self.prog.machines[mid.idx()].payload.equals(&PType::Null, &self.prog) {
            let msg = format!(
                "main machine {} cannot take an input parameter in the entry function of the start state.",
                main.name
            );
            return Err(self.e.no_main(span, &msg));
        }
        Ok(mid)
    }

    fn check_impl(&mut self, info: &ModuleInfo) -> Result<(), Fatal> {
        if let Some(i) = Self::not_closed_interface(info) {
            let msg = format!(
                "implementation module is not closed with respect to created interfaces; interface {} is created but not implemented inside the module",
                self.iface_name(i)
            );
            let span = self.prog.interfaces[i.idx()].span;
            return Err(self.e.not_closed(span, &msg));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Parametric tests
    // ------------------------------------------------------------------

    fn parse_seq_primitive(&self, p: &ast::SeqPrimitive) -> Result<(ParamValue, Span), Fatal> {
        Ok(match p {
            ast::SeqPrimitive::Bool(b, s) => (ParamValue::Bool(*b), *s),
            ast::SeqPrimitive::Int { negative, text, span } => {
                let Some(v) = parse_int_literal(text) else {
                    return Err(self.e.value_out_of_range(*span, "int"));
                };
                (ParamValue::Int(if *negative { -v } else { v }), *span)
            }
        })
    }

    fn param_specs(&mut self, params: &[ast::ParamBinding], params_span: Span) -> Result<Vec<ParamSpec>, Fatal> {
        // ParamExprVisitor: build the named tuple of sequence literals.
        let mut names: Vec<&str> = Vec::new();
        let mut parsed: Vec<(PType, Vec<ParamValue>, Span)> = Vec::new();
        for b in params {
            let mut values = Vec::new();
            let mut base: Option<PType> = None;
            let mut seen_ints: Vec<i64> = Vec::new();
            // upstream reports at the seqLiteralBody rule, which starts at the first value
            let mut body_span: Option<Span> = None;
            for p in &b.values {
                let (v, s) = self.parse_seq_primitive(p)?;
                let body_span = *body_span.get_or_insert(s);
                let ty = match v {
                    ParamValue::Int(_) => PType::Int,
                    ParamValue::Bool(_) => PType::Bool,
                };
                match &base {
                    None => base = Some(ty.clone()),
                    Some(bt) => {
                        if !ty.equals(bt, &self.prog) {
                            return Err(self.e.type_mismatch(s, &ty, &[bt], &self.prog));
                        }
                    }
                }
                if let ParamValue::Int(i) = v {
                    if seen_ints.contains(&i) {
                        let msg = format!(
                            "Invalid parameter list: Duplicate integer value '{}' found. All test values must be unique (e.g., [2, 2] is not allowed).",
                            i
                        );
                        return Err(self
                            .e
                            .internal_error(body_span, &msg)
                            .help("list each value once, e.g. `(n in [1, 2, 3])`"));
                    }
                    seen_ints.push(i);
                }
                values.push(v);
            }
            let base = base.unwrap_or(PType::Error);
            parsed.push((PType::seq(base), values, b.span));
            if names.contains(&b.name.name.as_str()) {
                return Err(self.e.duplicate_named_tuple_entry(b.name.span, &b.name.name));
            }
            names.push(&b.name.name);
        }
        // EnumerateParamAssignments
        let g = self.scopes.global();
        let mut specs = Vec::new();
        for (b, (ty, values, span)) in params.iter().zip(parsed) {
            let var =
                self.scopes.get(g).var_list.iter().copied().find(|v| self.prog.var_decls[v.idx()].name == b.name.name);
            let Some(var) = var else {
                return Err(self.e.undeclared_global_param(params_span, &b.name.name));
            };
            let expected = PType::seq(self.prog.var_decls[var.idx()].ty.clone());
            if !ty.equals(&expected, &self.prog) {
                return Err(self.e.type_mismatch(span, &ty, &[&expected], &self.prog));
            }
            specs.push(ParamSpec { var, values });
        }
        Ok(specs)
    }

    /// `SimpleExprEval.Eval` over the typed assume expression.
    fn simple_eval(&self, store: &[(VarId, ParamValue)], e: &Expr) -> Result<ParamValue, Fatal> {
        let bad = || {
            self.e
                .internal_error(e.span, "unsupported expression in test assumption")
                .help("use only params, literals, arithmetic, comparisons and `&&`, `||`, `!` in `assume`")
        };
        let int = |v: ParamValue| match v {
            ParamValue::Int(i) => Ok(i),
            _ => Err(bad()),
        };
        let boo = |v: ParamValue| match v {
            ParamValue::Bool(b) => Ok(b),
            _ => Err(bad()),
        };
        Ok(match &e.kind {
            ExprKind::BoolLit(b) => ParamValue::Bool(*b),
            ExprKind::IntLit(i) => ParamValue::Int(*i),
            ExprKind::Unary { op, operand } => {
                let v = self.simple_eval(store, operand)?;
                match op {
                    UnOp::Neg => ParamValue::Int(int(v)?.wrapping_neg()),
                    UnOp::Not => ParamValue::Bool(!boo(v)?),
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.simple_eval(store, lhs)?;
                let r = self.simple_eval(store, rhs)?;
                match op {
                    BinOp::Add => ParamValue::Int(int(l)?.wrapping_add(int(r)?)),
                    BinOp::Sub => ParamValue::Int(int(l)?.wrapping_sub(int(r)?)),
                    BinOp::Mul => ParamValue::Int(int(l)?.wrapping_mul(int(r)?)),
                    BinOp::Div => ParamValue::Int(int(l)?.checked_div(int(r)?).ok_or_else(bad)?),
                    BinOp::Mod => ParamValue::Int(int(l)?.checked_rem(int(r)?).ok_or_else(bad)?),
                    BinOp::Eq => ParamValue::Bool(match (l, r) {
                        (ParamValue::Int(a), ParamValue::Int(b)) => a == b,
                        (ParamValue::Bool(a), ParamValue::Bool(b)) => a == b,
                        _ => return Err(bad()),
                    }),
                    BinOp::Neq => ParamValue::Bool(match (l, r) {
                        (ParamValue::Int(a), ParamValue::Int(b)) => a != b,
                        (ParamValue::Bool(a), ParamValue::Bool(b)) => a != b,
                        _ => return Err(bad()),
                    }),
                    BinOp::Lt => ParamValue::Bool(int(l)? < int(r)?),
                    BinOp::Le => ParamValue::Bool(int(l)? <= int(r)?),
                    BinOp::Gt => ParamValue::Bool(int(l)? > int(r)?),
                    BinOp::Ge => ParamValue::Bool(int(l)? >= int(r)?),
                    BinOp::And => ParamValue::Bool(boo(l)? && boo(r)?),
                    BinOp::Or => ParamValue::Bool(boo(l)? || boo(r)?),
                    _ => return Err(bad()),
                }
            }
            ExprKind::Var(v) => store.iter().find(|(k, _)| k == v).map(|(_, v)| v.clone()).ok_or_else(bad)?,
            _ => return Err(bad()),
        })
    }

    /// `ParamAssignment.IterateAssignments`: enumerate assignments (first parameter varies
    /// fastest), filter by the assumption, then pick a greedy t-wise cover.
    fn expand_assignments(
        &self,
        specs: &[ParamSpec],
        assume: Option<&Expr>,
        twise: usize,
    ) -> Result<Vec<ParamAssignment>, Fatal> {
        let n = specs.len();
        let mut idx = vec![0usize; n];
        let mut assignments: Vec<Vec<usize>> = Vec::new();
        loop {
            let store: Vec<(VarId, ParamValue)> =
                specs.iter().zip(&idx).map(|(s, i)| (s.var, s.values[*i].clone())).collect();
            let keep = match assume {
                None => true,
                Some(a) => match self.simple_eval(&store, a)? {
                    ParamValue::Bool(b) => b,
                    _ => {
                        return Err(self
                            .e
                            .internal_error(a.span, "test assumption is not a boolean")
                            .help("write `assume` as a condition, e.g. `assume (n > 0)`"))
                    }
                },
            };
            if keep {
                assignments.push(idx.clone());
            }
            // Next()
            let mut advanced = false;
            for i in 0..n {
                idx[i] += 1;
                if idx[i] < specs[i].values.len() {
                    advanced = true;
                    break;
                }
                idx[i] = 0;
            }
            if !advanced {
                break;
            }
        }
        let chosen = greedy_coverage(&specs.iter().map(|s| s.values.len()).collect::<Vec<_>>(), &assignments, twise);
        Ok(chosen
            .into_iter()
            .map(|a| ParamAssignment {
                values: specs.iter().zip(&assignments[a]).map(|(s, i)| (s.var, s.values[*i].clone())).collect(),
            })
            .collect())
    }

    // ------------------------------------------------------------------
    // ModuleSystemDeclarations.PopulateAllModuleExprs + Analyzer steps 9-10
    // ------------------------------------------------------------------

    pub fn populate_module_system(&mut self) -> Result<(), Fatal> {
        // Step 8: resolve every module expression, in upstream order.
        let mut named: Vec<RMod> = Vec::new();
        for i in 0..self.module_src.len() {
            let (_, expr, _) = self.module_src[i];
            named.push(self.resolve_mod_expr(expr, 0)?);
        }
        let mut tests: Vec<(RMod, Vec<ParamSpec>, Option<Expr>, usize)> = Vec::new();
        for ti in 0..self.test_src.len() {
            let t = self.test_src[ti];
            let m = self.resolve_mod_expr(&t.expr, 0)?;
            let specs = match &t.params {
                Some((ps, span)) => self.param_specs(ps, *span)?,
                None => Vec::new(),
            };
            let assume = match &t.assume {
                Some(a) => {
                    let g = self.scopes.global();
                    let f = self.new_fun("assume", None, g, FunSrc::None, a.span);
                    Some(self.check_expr(Ctx { fun: f, machine: None, scope: g }, a)?)
                }
                None => None,
            };
            let twise = match &t.twise {
                None => specs.len(),
                Some(tw) => {
                    let (n, span) = match tw {
                        ast::Twise::Pairwise(span) => (2, span),
                        ast::Twise::N(text, span) => match parse_int_literal(text) {
                            Some(n) => (n, span),
                            None => return Err(self.e.value_out_of_range(*span, "int")),
                        },
                    };
                    let check = if n < 1 {
                        Some(format!("twise number {} is less than 1.", n))
                    } else if n as usize > specs.len() {
                        Some(format!("twise number {} is greater than {}.", n, specs.len()))
                    } else {
                        None
                    };
                    if let Some(msg) = check {
                        return Err(self.e.invalid_twise(*span, t.span, &msg));
                    }
                    n as usize
                }
            };
            tests.push((m, specs, assume, twise));
        }
        let mut refinements: Vec<(RMod, RMod)> = Vec::new();
        for i in 0..self.refinement_src.len() {
            let (_, lhs, rhs, _, _) = self.refinement_src[i];
            let l = self.resolve_mod_expr(lhs, 0)?;
            let r = self.resolve_mod_expr(rhs, 0)?;
            refinements.push((l, r));
        }
        let mut impls: Vec<RMod> = Vec::new();
        if !self.impl_src.is_empty() {
            for i in 0..self.impl_src.len() {
                let ast::TopDecl::Implementation { expr, .. } = self.impl_src[i] else { unreachable!() };
                impls.push(self.resolve_mod_expr(expr, 0)?);
            }
        } else if self.test_src.is_empty() && self.prog.machines.iter().any(|m| m.name == "Main") {
            {
                let g = self.scopes.global();
                let mut bindings = Vec::new();
                for (mi, m) in self.prog.machines.iter().enumerate() {
                    if m.is_spec() {
                        continue;
                    }
                    if let Some(i) = self.scopes.lookup_interface(g, &m.name) {
                        bindings.push((i, MachineId(mi as u32)));
                    }
                }
                let main = self.prog.find_machine("Main");
                self.prog.impls.push(ImplDecl {
                    name: "DefaultImpl".into(),
                    main,
                    module: ModuleInfo::default(),
                    span: Span::dummy(),
                });
                impls.push(RMod::Bind(bindings, Span::dummy()));
            }
        }

        // Step 9: well-formedness, in upstream order.
        for m in &named {
            self.module_info(m)?;
        }
        let mut test_infos = Vec::new();
        for (m, _, _, _) in &tests {
            test_infos.push(self.module_info(m)?);
        }
        let mut ref_infos = Vec::new();
        for (l, _) in &refinements {
            ref_infos.push((self.module_info(l)?, None));
        }
        for (i, (_, r)) in refinements.iter().enumerate() {
            ref_infos[i].1 = Some(self.module_info(r)?);
        }
        let mut impl_infos = Vec::new();
        for m in &impls {
            impl_infos.push(self.module_info(m)?);
        }

        // Step 10: implementation, safety test and refinement checks.
        for (i, info) in impl_infos.into_iter().enumerate() {
            self.check_impl(&info)?;
            if i < self.impl_src.len() {
                let ast::TopDecl::Implementation { main, .. } = self.impl_src[i] else { unreachable!() };
                self.prog.impls[i].main = main.as_ref().and_then(|m| self.prog.find_machine(&m.name));
            }
            self.prog.impls[i].module = info;
        }
        for (ti, ((_, specs, assume, twise), info)) in tests.into_iter().zip(test_infos).enumerate() {
            let t = self.test_src[ti];
            let main = self.check_safety_test(&info, &t.main, t.span)?;
            let assignments = self.expand_assignments(&specs, assume.as_ref(), twise)?;
            let td = &mut self.prog.tests[ti];
            td.main = main;
            td.module = info;
            td.assignments = assignments;
        }
        // only the first refinement test is examined: upstream rejects it either way
        if let Some((linfo, rinfo)) = ref_infos.into_iter().next() {
            let (_, _, _, span, main) = self.refinement_src[0].clone();
            let rinfo = rinfo.expect("rhs info");
            if let Some(i) = Self::not_closed_interface(&linfo) {
                let msg = format!("LHS test module is not closed with respect to created interfaces; interface {} is created but not implemented inside the module", self.iface_name(i));
                return Err(self.e.not_closed(span, &msg));
            }
            if !linfo
                .interface_def
                .iter()
                .any(|(_, m)| self.prog.machines[m.idx()].name == main && !self.prog.machines[m.idx()].is_spec())
            {
                let msg = format!("machine {} does not exist in the LHS test module", main);
                return Err(self.e.no_main(span, &msg));
            }
            if let Some(i) = Self::not_closed_interface(&rinfo) {
                let msg = format!("RHS test module is not closed with respect to created interfaces; interface {} is created but not implemented inside the module", self.iface_name(i));
                return Err(self.e.not_closed(span, &msg));
            }
            if !rinfo
                .interface_def
                .iter()
                .any(|(_, m)| self.prog.machines[m.idx()].name == main && !self.prog.machines[m.idx()].is_spec())
            {
                let msg = format!("machine {} does not exist in the RHS test module", main);
                return Err(self.e.no_main(span, &msg));
            }
            // Upstream: refinement checking is not implemented.
            return Err(self.e.internal_error(span, "refinement tests are not supported").help(
                "check the implementation against the spec with `test t [main=M]: assert Spec in { ... };` instead",
            ));
        }
        Ok(())
    }
}

/// `ParamAssignment.GreedyCoverageExplore`: indices of the assignments to keep.
fn greedy_coverage(universe: &[usize], assignments: &[Vec<usize>], twise: usize) -> Vec<usize> {
    let all: Vec<usize> = (0..assignments.len()).collect();
    if twise == universe.len() {
        return all;
    }
    // vectors: every value combination over every `twise`-subset of parameters
    let mut vectors: Vec<Vec<(usize, usize)>> = Vec::new();
    for combo in combinations(universe.len(), twise) {
        let mut result: Vec<Vec<(usize, usize)>> = vec![Vec::new()];
        for &p in &combo {
            let mut next = Vec::new();
            for v in 0..universe[p] {
                for r in &result {
                    let mut c = r.clone();
                    c.push((p, v));
                    next.push(c);
                }
            }
            result = next;
        }
        vectors.extend(result);
    }
    let mut coverage: Vec<(usize, Vec<usize>)> = all
        .iter()
        .map(|&a| {
            let covered: Vec<usize> = vectors
                .iter()
                .enumerate()
                .filter(|(_, vec)| vec.iter().all(|(p, v)| assignments[a][*p] == *v))
                .map(|(i, _)| i)
                .collect();
            (a, covered)
        })
        .collect();
    let mut obligations: Vec<usize> =
        (0..vectors.len()).filter(|v| coverage.iter().any(|(_, c)| c.contains(v))).collect();
    let mut result = Vec::new();
    while !obligations.is_empty() {
        let mut best = 0usize;
        for (i, (_, c)) in coverage.iter().enumerate() {
            if c.len() > coverage[best].1.len() {
                best = i;
            }
        }
        let (a, covered) = coverage.remove(best);
        obligations.retain(|o| !covered.contains(o));
        for (_, c) in coverage.iter_mut() {
            c.retain(|x| !covered.contains(x));
        }
        result.push(a);
    }
    result
}

fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
    fn go(start: usize, n: usize, k: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if k == 0 {
            out.push(cur.clone());
            return;
        }
        for i in start..n {
            cur.push(i);
            go(i + 1, n, k - 1, cur, out);
            cur.pop();
        }
    }
    let mut out = Vec::new();
    go(0, n, k, &mut Vec::new(), &mut out);
    out
}
