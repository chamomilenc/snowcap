use crate::example_topologies::{example_networks_scenario, Reps, Topology};
use crate::NetworkSelection;
use serde_json::{json, Value};
use snowcap::hard_policies::HardPolicy;
use snowcap::netsim::config::{Config, ConfigExpr, ConfigModifier};
use snowcap::netsim::route_map::{
    RouteMap, RouteMapDirection, RouteMapMatch, RouteMapMatchAsPath, RouteMapMatchClause,
    RouteMapSet, RouteMapState,
};
use snowcap::netsim::{printer, BgpSessionType, Network, NetworkDevice, Prefix, RouterId};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub fn export(network: NetworkSelection, output: String) -> Result<(), Box<dyn Error>> {
    let scenario = network.repr();
    let (net, final_config, hard_policy) = super::get_topo(network)?;
    let output_path = Path::new(&output);
    let stats = export_case(&scenario, net, final_config, hard_policy, output_path)?;

    println!("Exported SEER case to {}", output_path.display());
    println!("  routers: {}", stats.routers);
    println!("  external routers: {}", stats.external_routers);
    println!("  links: {}", stats.links);
    println!("  modifiers: {}", stats.modifiers);
    if stats.unsupported_modifiers > 0 {
        println!("  unsupported modifiers: {}", stats.unsupported_modifiers);
    }

    Ok(())
}

pub fn export_chain_gadget_dataset(
    output: String,
    initial_variant: usize,
    final_variant: Option<usize>,
) -> Result<(), Box<dyn Error>> {
    export_example_gadget_dataset(
        output,
        Topology::ChainGadget,
        "ChainGadget",
        gadget_repetitions(),
        initial_variant,
        final_variant,
    )
}

pub fn export_bipartite_gadget_dataset(
    output: String,
    initial_variant: usize,
    final_variant: Option<usize>,
) -> Result<(), Box<dyn Error>> {
    let resolved_final_variant = final_variant.unwrap_or(initial_variant);
    if initial_variant <= 1 || resolved_final_variant <= 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "BipartiteGadget requires initial_variant and final_variant to be greater than 1; got initial_variant={}, final_variant={}",
                initial_variant, resolved_final_variant
            ),
        )
        .into());
    }

    export_example_gadget_dataset(
        output,
        Topology::BipartiteGadget,
        "BipartiteGadget",
        gadget_repetitions(),
        initial_variant,
        final_variant,
    )
}

pub fn export_difficult_gadget_repeated_dataset(
    output: String,
    initial_variant: usize,
    final_variant: Option<usize>,
) -> Result<(), Box<dyn Error>> {
    export_example_gadget_dataset(
        output,
        Topology::DifficultGadgetRepeated,
        "DifficultGadgetRepeated",
        difficult_gadget_repeated_repetitions(),
        initial_variant,
        final_variant,
    )
}

fn export_example_gadget_dataset(
    output: String,
    topology: Topology,
    topology_name: &str,
    repetitions: Vec<Reps>,
    initial_variant: usize,
    final_variant: Option<usize>,
) -> Result<(), Box<dyn Error>> {
    let output_root = Path::new(&output);
    fs::create_dir_all(output_root)?;

    let summary_path = output_root.join("generation_summary.csv");
    let mut summary = File::create(&summary_path)?;
    writeln!(summary, "req,status,output_dir,message")?;

    let mut success_count = 0usize;
    let mut failure_count = 0usize;
    for repetition in repetitions {
        let req = repetition_count(repetition);
        let scenario = format!("{}, rep={}", topology_name, req);
        let output_path = output_root.join(format!("req_{:03}", req));
        let result = example_networks_scenario(
            topology.clone(),
            initial_variant,
            final_variant,
            Some(repetition),
        )
        .and_then(|(net, final_config, hard_policy)| {
            export_case(&scenario, net, final_config, hard_policy, &output_path)
        });

        match result {
            Ok(stats) => {
                success_count += 1;
                writeln!(
                    summary,
                    "{},OK,{},{}",
                    req,
                    csv_field(&output_path.display().to_string()),
                    csv_field(&format!(
                        "routers={},externalRouters={},links={},modifiers={},unsupported={}",
                        stats.routers,
                        stats.external_routers,
                        stats.links,
                        stats.modifiers,
                        stats.unsupported_modifiers
                    ))
                )?;
                println!(
                    "Exported {} req {} to {}",
                    topology_name,
                    req,
                    output_path.display()
                );
            }
            Err(error) => {
                failure_count += 1;
                writeln!(
                    summary,
                    "{},FAILED,{},{}",
                    req,
                    csv_field(&output_path.display().to_string()),
                    csv_field(&error.to_string())
                )?;
                println!("Failed {} req {}: {}", topology_name, req, error);
            }
        }
    }

    println!(
        "Exported {} SEER dataset to {}",
        topology_name,
        output_root.display()
    );
    println!("  success: {}", success_count);
    println!("  failure: {}", failure_count);
    println!("  summary: {}", summary_path.display());

    Ok(())
}

fn export_case(
    scenario: &str,
    net: Network,
    final_config: Config,
    hard_policy: HardPolicy,
    output_path: &Path,
) -> Result<ExportStats, Box<dyn Error>> {
    let initial_config = net.current_config().clone();
    let patch = initial_config.get_diff(&final_config);
    fs::create_dir_all(output_path)?;

    let context = ExportContext::new(&net, &initial_config, &final_config, &patch.modifiers);
    let unsupported = context.unsupported_modifiers(&net);

    write_json(
        output_path.join("topology.json"),
        topology_json(&net, &initial_config)?,
    )?;
    write_json(
        output_path.join("configuration.json"),
        configuration_json(&net, &initial_config, &final_config, &context)?,
    )?;
    write_json(
        output_path.join("updates.json"),
        updates_json(&net, &context)?,
    )?;
    write_specification(output_path.join("specification.ltl"), &net)?;
    write_json(
        output_path.join("metadata.json"),
        metadata_json(&net, scenario, &patch.modifiers, &unsupported),
    )?;
    write_json(
        output_path.join("snowcap.json"),
        snowcap_json(&net, &hard_policy, &patch.modifiers, &unsupported)?,
    )?;

    Ok(ExportStats {
        routers: net.get_routers().len(),
        external_routers: net.get_external_routers().len(),
        links: net.links_symmetric().len(),
        modifiers: patch.modifiers.len(),
        unsupported_modifiers: unsupported.len(),
    })
}

fn topology_json(net: &Network, config: &Config) -> Result<Value, Box<dyn Error>> {
    let mut routers = Vec::new();
    for id in sorted_ids(net.get_routers()) {
        routers.push(json!({
            "id": name(net, id)?,
            "asNumber": as_number(net, id)?,
            "protocols": ["BGP", "OSPF"],
            "external": false
        }));
    }
    for id in sorted_ids(net.get_external_routers()) {
        routers.push(json!({
            "id": name(net, id)?,
            "asNumber": as_number(net, id)?,
            "protocols": ["BGP"],
            "external": true
        }));
    }

    let ospf_costs = ospf_costs(net, config)?;
    let mut links = Vec::new();
    for (a, b) in net.links_symmetric() {
        let left = name(net, *a)?;
        let right = name(net, *b)?;
        let external = is_external(net, *a) || is_external(net, *b);
        let link_type = if external { "ebgp" } else { "ospf" };
        let id = link_id(link_type, &left, &right);
        let mut value = json!({
            "id": id,
            "type": link_type,
            "endpoints": [left, right]
        });
        if link_type == "ospf" {
            if let Some(cost) = ospf_costs.get(&(left.clone(), right.clone())) {
                value["cost"] = json!(*cost);
            }
        }
        links.push(value);
    }

    Ok(json!({ "topology": { "routers": routers, "links": links } }))
}

fn configuration_json(
    net: &Network,
    initial_config: &Config,
    final_config: &Config,
    context: &ExportContext,
) -> Result<Value, Box<dyn Error>> {
    let mut prefix_origins = Vec::new();
    for id in sorted_ids(net.get_external_routers()) {
        if let NetworkDevice::ExternalRouter(router) = net.get_device(id) {
            for route in router.get_advertised_routes() {
                prefix_origins.push(json!({
                    "prefix": prefix_name(route.prefix),
                    "originRouter": router.name(),
                    "originAs": router.as_id().0
                }));
            }
        }
    }
    prefix_origins.sort_by(|a, b| value_key(a).cmp(&value_key(b)));

    let mut ospf_links_by_id = BTreeMap::new();
    for expr in sorted_config_exprs(initial_config) {
        match expr {
            ConfigExpr::IgpLinkWeight {
                source,
                target,
                weight,
            } => {
                let left = name(net, *source)?;
                let right = name(net, *target)?;
                let id = link_id("ospf", &left, &right);
                ospf_links_by_id.insert(
                    id.clone(),
                    json!({
                        "linkId": id,
                        "cost": weight_to_int(*weight)
                    }),
                );
            }
            ConfigExpr::BgpSession { .. } | ConfigExpr::BgpRouteMap { .. } => {}
            ConfigExpr::StaticRoute { .. } => {}
        }
    }
    let ospf_links = ospf_links_by_id.into_values().collect::<Vec<_>>();

    let mut bgp_sessions = Vec::new();
    for (expr, enabled) in union_bgp_sessions(initial_config, final_config) {
        if let ConfigExpr::BgpSession {
            source,
            target,
            session_type,
        } = expr
        {
            bgp_sessions.push(bgp_session_json(
                net,
                *source,
                *target,
                *session_type,
                enabled,
            )?);
        }
    }

    let mut routing_policies = Vec::new();
    for expr in union_route_maps(initial_config, final_config) {
        if let ConfigExpr::BgpRouteMap {
            router,
            direction,
            map,
        } = expr
        {
            routing_policies.push(policy_json(net, *router, *direction, map, context)?);
        }
    }

    Ok(json!({
        "initialConfiguration": {
            "prefixOrigins": prefix_origins,
            "ospfLinks": ospf_links,
            "bgpSessions": bgp_sessions,
            "routingPolicies": routing_policies
        }
    }))
}

fn updates_json(net: &Network, context: &ExportContext) -> Result<Value, Box<dyn Error>> {
    let mut updates = Vec::new();
    for (index, modifier) in context.modifiers.iter().enumerate() {
        let id = format!("u{:04}_{}", index + 1, modifier_slug(modifier));
        if let Some(update) = update_json(net, context, modifier, &id)? {
            updates.push(update);
        }
    }
    Ok(json!({ "updates": updates }))
}

fn update_json(
    net: &Network,
    context: &ExportContext,
    modifier: &ConfigModifier,
    id: &str,
) -> Result<Option<Value>, Box<dyn Error>> {
    match modifier {
        ConfigModifier::Insert(expr) => insert_update_json(net, context, expr, id),
        ConfigModifier::Remove(expr) => remove_update_json(net, context, expr, id),
        ConfigModifier::Update { from, to } => update_expr_json(net, context, from, to, id),
    }
}

fn insert_update_json(
    net: &Network,
    context: &ExportContext,
    expr: &ConfigExpr,
    id: &str,
) -> Result<Option<Value>, Box<dyn Error>> {
    match expr {
        ConfigExpr::BgpSession {
            source,
            target,
            session_type,
        } => {
            let session_id = context.session_id(net, *source, *target);
            let (relationship, reflector, client) =
                session_relationship(net, *source, *target, *session_type)?;
            let mut after = json!({
                "enabled": true,
                "relationship": relationship
            });
            if let Some(rr) = &reflector {
                after["routeReflector"] = json!(rr);
            }
            if let Some(c) = &client {
                after["client"] = json!(c);
            }
            Ok(Some(json!({
                "id": id,
                "type": if relationship == "peer" { "sessionStateChange" } else { "sessionRelationshipChange" },
                "description": printer::config_modifier(net, &ConfigModifier::Insert(expr.clone()))?,
                "target": {
                    "sessionId": session_id,
                    "routeReflector": reflector,
                    "client": client
                },
                "before": { "enabled": false },
                "after": after
            })))
        }
        ConfigExpr::BgpRouteMap {
            router,
            direction,
            map,
        } => {
            let policy_id = context.policy_id(net, *router, *direction, map);
            let clause = clause_json(net, map)?;
            Ok(Some(json!({
                "id": id,
                "type": "routingPolicyActionChange",
                "description": printer::config_modifier(net, &ConfigModifier::Insert(expr.clone()))?,
                "target": {
                    "policyId": policy_id,
                    "router": name(net, *router)?,
                    "neighbor": neighbor_name(net, map)?,
                    "clauseId": clause["id"].as_str().unwrap_or("clause")
                },
                "before": { "action": "permit" },
                "after": update_value_from_clause(&clause)
            })))
        }
        ConfigExpr::IgpLinkWeight {
            source,
            target,
            weight,
        } => {
            let left = name(net, *source)?;
            let right = name(net, *target)?;
            Ok(Some(json!({
                "id": id,
                "type": "ospfCostChange",
                "description": printer::config_modifier(net, &ConfigModifier::Insert(expr.clone()))?,
                "target": { "linkId": link_id("ospf", &left, &right) },
                "before": { "cost": 1000000 },
                "after": { "cost": weight_to_int(*weight) }
            })))
        }
        ConfigExpr::StaticRoute { .. } => Ok(None),
    }
}

fn remove_update_json(
    net: &Network,
    context: &ExportContext,
    expr: &ConfigExpr,
    id: &str,
) -> Result<Option<Value>, Box<dyn Error>> {
    match expr {
        ConfigExpr::BgpSession {
            source,
            target,
            session_type,
        } => {
            let session_id = context.session_id(net, *source, *target);
            let (relationship, reflector, client) =
                session_relationship(net, *source, *target, *session_type)?;
            let mut before = json!({
                "enabled": true,
                "relationship": relationship
            });
            if let Some(rr) = &reflector {
                before["routeReflector"] = json!(rr);
            }
            if let Some(c) = &client {
                before["client"] = json!(c);
            }
            Ok(Some(json!({
                "id": id,
                "type": "sessionStateChange",
                "description": printer::config_modifier(net, &ConfigModifier::Remove(expr.clone()))?,
                "target": {
                    "sessionId": session_id,
                    "routeReflector": reflector,
                    "client": client
                },
                "before": before,
                "after": { "enabled": false }
            })))
        }
        ConfigExpr::BgpRouteMap {
            router,
            direction,
            map,
        } => {
            let policy_id = context.policy_id(net, *router, *direction, map);
            let clause = clause_json(net, map)?;
            Ok(Some(json!({
                "id": id,
                "type": "routingPolicyActionChange",
                "description": printer::config_modifier(net, &ConfigModifier::Remove(expr.clone()))?,
                "target": {
                    "policyId": policy_id,
                    "router": name(net, *router)?,
                    "neighbor": neighbor_name(net, map)?,
                    "clauseId": clause["id"].as_str().unwrap_or("clause")
                },
                "before": update_value_from_clause(&clause),
                "after": { "action": "permit" }
            })))
        }
        ConfigExpr::IgpLinkWeight {
            source,
            target,
            weight,
        } => {
            let left = name(net, *source)?;
            let right = name(net, *target)?;
            Ok(Some(json!({
                "id": id,
                "type": "ospfCostChange",
                "description": printer::config_modifier(net, &ConfigModifier::Remove(expr.clone()))?,
                "target": { "linkId": link_id("ospf", &left, &right) },
                "before": { "cost": weight_to_int(*weight) },
                "after": { "cost": 1000000 }
            })))
        }
        ConfigExpr::StaticRoute { .. } => Ok(None),
    }
}

fn update_expr_json(
    net: &Network,
    context: &ExportContext,
    from: &ConfigExpr,
    to: &ConfigExpr,
    id: &str,
) -> Result<Option<Value>, Box<dyn Error>> {
    match (from, to) {
        (
            ConfigExpr::IgpLinkWeight {
                source,
                target,
                weight: before,
            },
            ConfigExpr::IgpLinkWeight { weight: after, .. },
        ) => {
            let left = name(net, *source)?;
            let right = name(net, *target)?;
            Ok(Some(json!({
                "id": id,
                "type": "ospfCostChange",
                "description": printer::config_modifier(net, &ConfigModifier::Update { from: from.clone(), to: to.clone() })?,
                "target": { "linkId": link_id("ospf", &left, &right) },
                "before": { "cost": weight_to_int(*before) },
                "after": { "cost": weight_to_int(*after) }
            })))
        }
        (
            ConfigExpr::BgpSession {
                source,
                target,
                session_type: before,
            },
            ConfigExpr::BgpSession {
                session_type: after,
                ..
            },
        ) => {
            let session_id = context.session_id(net, *source, *target);
            let (before_rel, before_rr, before_client) =
                session_relationship(net, *source, *target, *before)?;
            let (after_rel, after_rr, after_client) =
                session_relationship(net, *source, *target, *after)?;
            Ok(Some(json!({
                "id": id,
                "type": "sessionRelationshipChange",
                "description": printer::config_modifier(net, &ConfigModifier::Update { from: from.clone(), to: to.clone() })?,
                "target": {
                    "sessionId": session_id,
                    "routeReflector": after_rr,
                    "client": after_client
                },
                "before": {
                    "enabled": true,
                    "relationship": before_rel,
                    "routeReflector": before_rr,
                    "client": before_client
                },
                "after": {
                    "enabled": true,
                    "relationship": after_rel,
                    "routeReflector": after_rr,
                    "client": after_client
                }
            })))
        }
        (
            ConfigExpr::BgpRouteMap {
                router,
                direction,
                map: before,
            },
            ConfigExpr::BgpRouteMap { map: after, .. },
        ) => {
            let policy_id = context.policy_id(net, *router, *direction, before);
            let before_clause = clause_json(net, before)?;
            let after_clause = clause_json(net, after)?;
            Ok(Some(json!({
                "id": id,
                "type": "routingPolicySetChange",
                "description": printer::config_modifier(net, &ConfigModifier::Update { from: from.clone(), to: to.clone() })?,
                "target": {
                    "policyId": policy_id,
                    "router": name(net, *router)?,
                    "neighbor": neighbor_name(net, before)?,
                    "clauseId": before_clause["id"].as_str().unwrap_or("clause")
                },
                "before": update_value_from_clause(&before_clause),
                "after": update_value_from_clause(&after_clause)
            })))
        }
        _ => Ok(None),
    }
}

fn bgp_session_json(
    net: &Network,
    source: RouterId,
    target: RouterId,
    session_type: BgpSessionType,
    enabled: bool,
) -> Result<Value, Box<dyn Error>> {
    let left = name(net, source)?;
    let right = name(net, target)?;
    let (relationship, route_reflector, client) =
        session_relationship(net, source, target, session_type)?;
    let mut value = json!({
        "id": session_id(&left, &right),
        "type": if session_type.is_ebgp() { "eBGP" } else { "iBGP" },
        "endpoints": [left, right],
        "enabled": enabled,
        "relationship": relationship
    });
    if let Some(rr) = route_reflector {
        value["routeReflector"] = json!(rr);
    }
    if let Some(c) = client {
        value["client"] = json!(c);
    }
    Ok(value)
}

fn policy_json(
    net: &Network,
    router: RouterId,
    direction: RouteMapDirection,
    map: &RouteMap,
    context: &ExportContext,
) -> Result<Value, Box<dyn Error>> {
    let neighbor = neighbor_name(net, map)?;
    Ok(json!({
        "id": context.policy_id(net, router, direction, map),
        "router": name(net, router)?,
        "direction": direction_name(direction),
        "neighbor": neighbor,
        "clauses": [clause_json(net, map)?]
    }))
}

fn clause_json(net: &Network, map: &RouteMap) -> Result<Value, Box<dyn Error>> {
    let mut match_value = json!({});
    if map.conds().is_empty() {
        match_value["any"] = json!(true);
    }
    for cond in map.conds() {
        match cond {
            RouteMapMatch::Neighbor(_) => {}
            RouteMapMatch::Prefix(RouteMapMatchClause::Equal(prefix)) => {
                match_value["prefix"] = json!(prefix_name(*prefix));
            }
            RouteMapMatch::Prefix(_) => {
                match_value["any"] = json!(true);
            }
            RouteMapMatch::AsPath(RouteMapMatchAsPath::Contains(as_id)) => {
                match_value["asPathContainsRouter"] = json!(format!("as{}", as_id.0));
            }
            RouteMapMatch::AsPath(_) => {
                match_value["any"] = json!(true);
            }
            RouteMapMatch::NextHop(next_hop) => {
                match_value["nextHop"] = json!(name(net, *next_hop)?);
            }
            RouteMapMatch::Community(_) => {
                match_value["any"] = json!(true);
            }
        }
    }

    let mut clause = json!({
        "id": format!("clause_{}", map.order()),
        "match": match_value,
        "action": if map.state() == RouteMapState::Deny { "deny" } else { "permit" }
    });
    let set_value = set_json(map);
    if !set_value.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        clause["set"] = set_value;
    }
    Ok(clause)
}

fn set_json(map: &RouteMap) -> Value {
    let mut set = json!({});
    for action in map.actions() {
        match action {
            RouteMapSet::LocalPref(value) => {
                set["localPreference"] = json!(value.unwrap_or(100));
            }
            RouteMapSet::Med(value) => {
                set["med"] = json!(value.unwrap_or(0));
            }
            RouteMapSet::Community(value) => {
                set["community"] = json!(value.unwrap_or(0));
            }
            RouteMapSet::IgpCost(value) => {
                set["igpCost"] = json!(weight_to_int(*value));
            }
            RouteMapSet::NextHop(router) => {
                set["nextHopRouterIndex"] = json!(router.index());
            }
        }
    }
    set
}

fn update_value_from_clause(clause: &Value) -> Value {
    let mut value = json!({
        "action": clause["action"].as_str().unwrap_or("permit")
    });
    if let Some(set) = clause.get("set") {
        value["set"] = set.clone();
    }
    value
}

fn write_specification(path: impl AsRef<Path>, net: &Network) -> Result<(), Box<dyn Error>> {
    let routers = sorted_ids(net.get_routers())
        .into_iter()
        .map(|id| name(net, id))
        .collect::<Result<Vec<_>, _>>()?;
    let prefixes = sorted_prefixes(net.get_known_prefixes().iter().copied())
        .into_iter()
        .map(prefix_name)
        .collect::<Vec<_>>();

    let mut contents = String::new();
    for prefix in &prefixes {
        contents.push_str(&format!(
            "hard reachability_{}:\n  always forall r in {{{}}}: reachable(route(r, {}));\n\n",
            prefix,
            routers.join(", "),
            prefix
        ));
    }
    contents.push_str(&format!(
        "soft minimize_traffic_shift:\n  minimize trafficShift(\n    routers = {{{}}},\n    prefixes = {{{}}},\n    compare = previous\n  );\n",
        routers.join(", "),
        prefixes.join(", ")
    ));
    fs::write(path, contents)?;
    Ok(())
}

fn metadata_json(
    net: &Network,
    scenario: &str,
    modifiers: &[ConfigModifier],
    unsupported: &[String],
) -> Value {
    json!({
        "schemaVersion": "seer-snowcap-export-v1",
        "source": "snowcap",
        "scenario": scenario,
        "numRouters": net.get_routers().len(),
        "numExternalRouters": net.get_external_routers().len(),
        "numEdges": net.links_symmetric().len(),
        "numPrefixes": net.get_known_prefixes().len(),
        "numCommands": modifiers.len(),
        "numUnsupportedModifiers": unsupported.len(),
        "unsupportedModifiers": unsupported
    })
}

fn snowcap_json(
    net: &Network,
    hard_policy: &HardPolicy,
    modifiers: &[ConfigModifier],
    unsupported: &[String],
) -> Result<Value, Box<dyn Error>> {
    let mut rendered = Vec::new();
    for modifier in modifiers {
        rendered.push(printer::config_modifier(net, modifier)?);
    }
    Ok(json!({
        "hardPolicy": format!("{:?}", hard_policy),
        "modifiers": rendered,
        "unsupportedModifiers": unsupported
    }))
}

fn write_json(path: impl AsRef<Path>, value: Value) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &value)?;
    Ok(())
}

fn gadget_repetitions() -> Vec<Reps> {
    vec![
        Reps::Rep1,
        Reps::Rep2,
        Reps::Rep3,
        Reps::Rep4,
        Reps::Rep5,
        Reps::Rep6,
        Reps::Rep7,
        Reps::Rep8,
        Reps::Rep9,
        Reps::Rep10,
        Reps::Rep11,
        Reps::Rep12,
        Reps::Rep13,
        Reps::Rep14,
        Reps::Rep15,
        Reps::Rep16,
        Reps::Rep17,
        Reps::Rep18,
        Reps::Rep19,
        Reps::Rep20,
        Reps::Rep30,
        Reps::Rep40,
        Reps::Rep50,
        Reps::Rep60,
        Reps::Rep70,
        Reps::Rep80,
        Reps::Rep90,
        Reps::Rep100,
    ]
}

fn difficult_gadget_repeated_repetitions() -> Vec<Reps> {
    vec![
        Reps::Rep1,
        Reps::Rep2,
        Reps::Rep3,
        Reps::Rep4,
        Reps::Rep5,
        Reps::Rep6,
        Reps::Rep7,
        Reps::Rep8,
        Reps::Rep9,
        Reps::Rep10,
        Reps::Rep11,
        Reps::Rep12,
        Reps::Rep13,
        Reps::Rep14,
        Reps::Rep15,
        Reps::Rep16,
        Reps::Rep17,
        Reps::Rep18,
        Reps::Rep19,
        Reps::Rep20,
    ]
}

fn repetition_count(repetition: Reps) -> usize {
    match repetition {
        Reps::Rep1 => 1,
        Reps::Rep2 => 2,
        Reps::Rep3 => 3,
        Reps::Rep4 => 4,
        Reps::Rep5 => 5,
        Reps::Rep6 => 6,
        Reps::Rep7 => 7,
        Reps::Rep8 => 8,
        Reps::Rep9 => 9,
        Reps::Rep10 => 10,
        Reps::Rep11 => 11,
        Reps::Rep12 => 12,
        Reps::Rep13 => 13,
        Reps::Rep14 => 14,
        Reps::Rep15 => 15,
        Reps::Rep16 => 16,
        Reps::Rep17 => 17,
        Reps::Rep18 => 18,
        Reps::Rep19 => 19,
        Reps::Rep20 => 20,
        Reps::Rep30 => 30,
        Reps::Rep40 => 40,
        Reps::Rep50 => 50,
        Reps::Rep60 => 60,
        Reps::Rep70 => 70,
        Reps::Rep80 => 80,
        Reps::Rep90 => 90,
        Reps::Rep100 => 100,
    }
}

fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

struct ExportStats {
    routers: usize,
    external_routers: usize,
    links: usize,
    modifiers: usize,
    unsupported_modifiers: usize,
}

fn sorted_ids(mut ids: Vec<RouterId>) -> Vec<RouterId> {
    ids.sort_by_key(|id| id.index());
    ids
}

fn sorted_prefixes(prefixes: impl Iterator<Item = Prefix>) -> Vec<Prefix> {
    let mut prefixes = prefixes.collect::<Vec<_>>();
    prefixes.sort_by_key(|prefix| prefix.0);
    prefixes
}

fn sorted_config_exprs(config: &Config) -> Vec<&ConfigExpr> {
    let mut expressions = config.iter().collect::<Vec<_>>();
    expressions.sort_by_key(|expr| format!("{:?}", expr.key()));
    expressions
}

fn union_bgp_sessions<'a>(
    initial_config: &'a Config,
    final_config: &'a Config,
) -> Vec<(&'a ConfigExpr, bool)> {
    let mut sessions = BTreeMap::new();
    for expr in sorted_config_exprs(final_config) {
        if let ConfigExpr::BgpSession { source, target, .. } = expr {
            sessions.insert(router_pair_key(*source, *target), (expr, false));
        }
    }
    for expr in sorted_config_exprs(initial_config) {
        if let ConfigExpr::BgpSession { source, target, .. } = expr {
            sessions.insert(router_pair_key(*source, *target), (expr, true));
        }
    }
    sessions.into_values().collect()
}

fn union_route_maps<'a>(
    initial_config: &'a Config,
    final_config: &'a Config,
) -> Vec<&'a ConfigExpr> {
    let mut maps = BTreeMap::new();
    for expr in sorted_config_exprs(final_config) {
        if let ConfigExpr::BgpRouteMap {
            router,
            direction,
            map,
        } = expr
        {
            maps.insert(
                (
                    (*router).index(),
                    direction_name(*direction).to_string(),
                    map.order(),
                ),
                expr,
            );
        }
    }
    for expr in sorted_config_exprs(initial_config) {
        if let ConfigExpr::BgpRouteMap {
            router,
            direction,
            map,
        } = expr
        {
            maps.insert(
                (
                    (*router).index(),
                    direction_name(*direction).to_string(),
                    map.order(),
                ),
                expr,
            );
        }
    }
    maps.into_values().collect()
}

fn name(net: &Network, id: RouterId) -> Result<String, Box<dyn Error>> {
    Ok(net.get_router_name(id)?.to_string())
}

fn as_number(net: &Network, id: RouterId) -> Result<u32, Box<dyn Error>> {
    match net.get_device(id) {
        NetworkDevice::InternalRouter(router) => Ok(router.as_id().0),
        NetworkDevice::ExternalRouter(router) => Ok(router.as_id().0),
        NetworkDevice::None => Err(format!("Missing router {}", id.index()).into()),
    }
}

fn is_external(net: &Network, id: RouterId) -> bool {
    matches!(net.get_device(id), NetworkDevice::ExternalRouter(_))
}

fn prefix_name(prefix: Prefix) -> String {
    format!("p{}", prefix.0)
}

fn weight_to_int(weight: f32) -> i32 {
    if weight.is_infinite() {
        1_000_000
    } else {
        weight.round() as i32
    }
}

fn link_id(kind: &str, left: &str, right: &str) -> String {
    let mut endpoints = [left.to_string(), right.to_string()];
    endpoints.sort();
    format!("{}_{}_{}", kind, endpoints[0], endpoints[1])
}

fn session_id(left: &str, right: &str) -> String {
    link_id("bgp", left, right)
}

fn session_relationship(
    net: &Network,
    source: RouterId,
    target: RouterId,
    session_type: BgpSessionType,
) -> Result<(&'static str, Option<String>, Option<String>), Box<dyn Error>> {
    match session_type {
        BgpSessionType::EBgp => Ok(("peer", None, None)),
        BgpSessionType::IBgpPeer => Ok(("peer", None, None)),
        BgpSessionType::IBgpClient => Ok((
            "routeReflectorClient",
            Some(name(net, source)?),
            Some(name(net, target)?),
        )),
    }
}

fn direction_name(direction: RouteMapDirection) -> &'static str {
    match direction {
        RouteMapDirection::Incoming => "import",
        RouteMapDirection::Outgoing => "export",
    }
}

fn neighbor_name(net: &Network, map: &RouteMap) -> Result<String, Box<dyn Error>> {
    if let Some(neighbor) = map.match_neighbor() {
        name(net, neighbor)
    } else {
        Ok("*".to_string())
    }
}

fn ospf_costs(
    net: &Network,
    config: &Config,
) -> Result<BTreeMap<(String, String), i32>, Box<dyn Error>> {
    let mut costs = BTreeMap::new();
    for expr in config.iter() {
        if let ConfigExpr::IgpLinkWeight {
            source,
            target,
            weight,
        } = expr
        {
            let left = name(net, *source)?;
            let right = name(net, *target)?;
            costs.insert((left.clone(), right.clone()), weight_to_int(*weight));
            costs.insert((right, left), weight_to_int(*weight));
        }
    }
    Ok(costs)
}

fn modifier_slug(modifier: &ConfigModifier) -> String {
    match modifier {
        ConfigModifier::Insert(expr) => format!("insert_{}", expr_slug(expr)),
        ConfigModifier::Remove(expr) => format!("remove_{}", expr_slug(expr)),
        ConfigModifier::Update { to, .. } => format!("update_{}", expr_slug(to)),
    }
}

fn expr_slug(expr: &ConfigExpr) -> String {
    match expr {
        ConfigExpr::IgpLinkWeight { source, target, .. } => {
            format!("ospf_{}_{}", source.index(), target.index())
        }
        ConfigExpr::BgpSession { source, target, .. } => {
            format!("bgp_{}_{}", source.index(), target.index())
        }
        ConfigExpr::BgpRouteMap {
            router,
            direction,
            map,
        } => {
            format!(
                "policy_{}_{}_{}",
                router.index(),
                direction_name(*direction),
                map.order()
            )
        }
        ConfigExpr::StaticRoute { router, prefix, .. } => {
            format!("static_{}_{}", router.index(), prefix.0)
        }
    }
}

fn value_key(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

struct ExportContext<'a> {
    modifiers: &'a [ConfigModifier],
    session_ids: BTreeMap<(usize, usize), String>,
    policy_ids: BTreeMap<(usize, String, usize), String>,
}

impl<'a> ExportContext<'a> {
    fn new(
        net: &Network,
        initial_config: &'a Config,
        final_config: &'a Config,
        modifiers: &'a [ConfigModifier],
    ) -> Self {
        let mut context = Self {
            modifiers,
            session_ids: BTreeMap::new(),
            policy_ids: BTreeMap::new(),
        };
        for config in [initial_config, final_config] {
            for expr in config.iter() {
                context.observe_expr(net, expr);
            }
        }
        for modifier in modifiers {
            match modifier {
                ConfigModifier::Insert(expr) | ConfigModifier::Remove(expr) => {
                    context.observe_expr(net, expr);
                }
                ConfigModifier::Update { from, to } => {
                    context.observe_expr(net, from);
                    context.observe_expr(net, to);
                }
            }
        }
        context
    }

    fn observe_expr(&mut self, net: &Network, expr: &ConfigExpr) {
        match expr {
            ConfigExpr::BgpSession { source, target, .. } => {
                let left = net.get_router_name(*source).unwrap_or("unknown");
                let right = net.get_router_name(*target).unwrap_or("unknown");
                self.session_ids
                    .insert(router_pair_key(*source, *target), session_id(left, right));
            }
            ConfigExpr::BgpRouteMap {
                router,
                direction,
                map,
            } => {
                self.policy_ids.insert(
                    (
                        (*router).index(),
                        direction_name(*direction).to_string(),
                        map.order(),
                    ),
                    format!(
                        "policy_{}_{}_{}",
                        net.get_router_name(*router).unwrap_or("unknown"),
                        direction_name(*direction),
                        map.order()
                    ),
                );
            }
            _ => {}
        }
    }

    fn session_id(&self, net: &Network, left: RouterId, right: RouterId) -> String {
        self.session_ids
            .get(&router_pair_key(left, right))
            .cloned()
            .unwrap_or_else(|| {
                session_id(
                    net.get_router_name(left).unwrap_or("unknown"),
                    net.get_router_name(right).unwrap_or("unknown"),
                )
            })
    }

    fn policy_id(
        &self,
        net: &Network,
        router: RouterId,
        direction: RouteMapDirection,
        map: &RouteMap,
    ) -> String {
        self.policy_ids
            .get(&(
                router.index(),
                direction_name(direction).to_string(),
                map.order(),
            ))
            .cloned()
            .unwrap_or_else(|| {
                format!(
                    "policy_{}_{}_{}",
                    net.get_router_name(router).unwrap_or("unknown"),
                    direction_name(direction),
                    map.order()
                )
            })
    }

    fn unsupported_modifiers(&self, net: &Network) -> Vec<String> {
        let mut unsupported = Vec::new();
        for modifier in self.modifiers {
            if !modifier_supported(modifier) {
                unsupported.push(
                    printer::config_modifier(net, modifier)
                        .unwrap_or_else(|_| format!("{:?}", modifier)),
                );
            }
        }
        unsupported
    }
}

fn router_pair_key(left: RouterId, right: RouterId) -> (usize, usize) {
    let a = left.index();
    let b = right.index();
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn modifier_supported(modifier: &ConfigModifier) -> bool {
    match modifier {
        ConfigModifier::Insert(expr) | ConfigModifier::Remove(expr) => expr_supported(expr),
        ConfigModifier::Update { from, to } => expr_supported(from) && expr_supported(to),
    }
}

fn expr_supported(expr: &ConfigExpr) -> bool {
    match expr {
        ConfigExpr::IgpLinkWeight { .. } | ConfigExpr::BgpSession { .. } => true,
        ConfigExpr::BgpRouteMap { map, .. } => route_map_supported(map),
        ConfigExpr::StaticRoute { .. } => false,
    }
}

fn route_map_supported(map: &RouteMap) -> bool {
    let supported_conds = map.conds().iter().all(|cond| {
        matches!(
            cond,
            RouteMapMatch::Neighbor(_)
                | RouteMapMatch::Prefix(RouteMapMatchClause::Equal(_))
                | RouteMapMatch::AsPath(RouteMapMatchAsPath::Contains(_))
        )
    });
    let supported_sets = map
        .actions()
        .iter()
        .all(|action| matches!(action, RouteMapSet::LocalPref(_)));
    supported_conds && supported_sets
}
