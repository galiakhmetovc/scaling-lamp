import assert from "node:assert/strict";
import { test } from "node:test";
import {
  blankSessionOutputRouteDraft,
  deliveryTargetCreateOptionsFromDraft,
  sessionOutputRouteCreateOptionsFromDraft
} from "./routeForms.ts";

test("deliveryTargetCreateOptionsFromDraft normalizes lists and JSON", () => {
  const options = deliveryTargetCreateOptionsFromDraft({
    target_id: "ops",
    kind: "telegram",
    address: "-100",
    scope: "monitor",
    owner_user_id: "",
    allowed_agent_ids: "default\njudge",
    allowed_session_ids: "session-a, session-b",
    send_policy_json: "{\"retry\":true}",
    format_policy: "summary"
  });

  assert.deepEqual(options, {
    kind: "telegram",
    address: "-100",
    scope: "monitor",
    owner_user_id: null,
    allowed_agent_ids: ["default", "judge"],
    allowed_session_ids: ["session-a", "session-b"],
    send_policy_json: "{\"retry\":true}",
    format_policy: "summary"
  });
});

test("sessionOutputRouteCreateOptionsFromDraft keeps optional route id nullable", () => {
  assert.deepEqual(sessionOutputRouteCreateOptionsFromDraft(blankSessionOutputRouteDraft("s", "t")), {
    route_id: null,
    filter_json: "null",
    format_policy: "full_text",
    enabled: true
  });
});
