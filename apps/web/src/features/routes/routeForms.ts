import type {
  DeliveryTarget,
  DeliveryTargetCreateOptions,
  DeliveryTargetUpdatePatch,
  SessionOutputRoute,
  SessionOutputRouteCreateOptions,
  SessionOutputRouteUpdatePatch
} from "../../types";

export type DeliveryTargetDraft = {
  target_id: string;
  kind: string;
  address: string;
  scope: string;
  owner_user_id: string;
  allowed_agent_ids: string;
  allowed_session_ids: string;
  send_policy_json: string;
  format_policy: string;
};

export type SessionOutputRouteDraft = {
  route_id: string;
  session_id: string;
  target_id: string;
  filter_json: string;
  format_policy: string;
  enabled: boolean;
};

export function blankDeliveryTargetDraft(): DeliveryTargetDraft {
  return {
    target_id: "",
    kind: "telegram",
    address: "",
    scope: "workspace",
    owner_user_id: "",
    allowed_agent_ids: "",
    allowed_session_ids: "",
    send_policy_json: "{}",
    format_policy: "full_text"
  };
}

export function deliveryTargetDraftFromTarget(target: DeliveryTarget): DeliveryTargetDraft {
  return {
    target_id: target.target_id,
    kind: target.kind,
    address: target.address,
    scope: target.scope,
    owner_user_id: target.owner_user_id ?? "",
    allowed_agent_ids: (target.allowed_agent_ids ?? []).join("\n"),
    allowed_session_ids: (target.allowed_session_ids ?? []).join("\n"),
    send_policy_json: target.send_policy_json ?? "{}",
    format_policy: target.format_policy
  };
}

export function deliveryTargetCreateOptionsFromDraft(draft: DeliveryTargetDraft): DeliveryTargetCreateOptions {
  return {
    kind: draft.kind.trim(),
    address: draft.address.trim(),
    scope: draft.scope.trim(),
    owner_user_id: draft.owner_user_id.trim() || null,
    allowed_agent_ids: parseList(draft.allowed_agent_ids),
    allowed_session_ids: parseList(draft.allowed_session_ids),
    send_policy_json: normalizeJson(draft.send_policy_json),
    format_policy: draft.format_policy.trim()
  };
}

export function deliveryTargetPatchFromDraft(draft: DeliveryTargetDraft): DeliveryTargetUpdatePatch {
  return deliveryTargetCreateOptionsFromDraft(draft);
}

export function blankSessionOutputRouteDraft(sessionId = "", targetId = ""): SessionOutputRouteDraft {
  return {
    route_id: "",
    session_id: sessionId,
    target_id: targetId,
    filter_json: "null",
    format_policy: "full_text",
    enabled: true
  };
}

export function sessionOutputRouteDraftFromRoute(route: SessionOutputRoute): SessionOutputRouteDraft {
  return {
    route_id: route.route_id,
    session_id: route.session_id,
    target_id: route.target_id,
    filter_json: route.filter_json,
    format_policy: route.format_policy,
    enabled: route.enabled
  };
}

export function sessionOutputRouteCreateOptionsFromDraft(draft: SessionOutputRouteDraft): SessionOutputRouteCreateOptions {
  return {
    route_id: draft.route_id.trim() || null,
    filter_json: normalizeJson(draft.filter_json),
    format_policy: draft.format_policy.trim(),
    enabled: draft.enabled
  };
}

export function sessionOutputRoutePatchFromDraft(draft: SessionOutputRouteDraft): SessionOutputRouteUpdatePatch {
  return {
    filter_json: normalizeJson(draft.filter_json),
    format_policy: draft.format_policy.trim(),
    enabled: draft.enabled
  };
}

function parseList(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function normalizeJson(value: string): string {
  const trimmed = value.trim() || "null";
  JSON.parse(trimmed);
  return trimmed;
}
