import assert from "node:assert/strict";
import test from "node:test";
import { isAuthorizedRequest } from "./server.mjs";

function request(authorization) {
  return {
    headers: authorization ? { authorization } : {}
  };
}

function basic(username, password) {
  return `Basic ${Buffer.from(`${username}:${password}`, "utf8").toString("base64")}`;
}

test("web auth is disabled when credentials are not configured", () => {
  assert.equal(
    isAuthorizedRequest(request(), {
      enabled: false,
      username: "",
      password: ""
    }),
    true
  );
});

test("web auth accepts valid basic credentials", () => {
  assert.equal(
    isAuthorizedRequest(request(basic("admin", "secret")), {
      enabled: true,
      username: "admin",
      password: "secret"
    }),
    true
  );
});

test("web auth rejects missing, malformed, or wrong credentials", () => {
  const auth = { enabled: true, username: "admin", password: "secret" };

  assert.equal(isAuthorizedRequest(request(), auth), false);
  assert.equal(isAuthorizedRequest(request("Bearer token"), auth), false);
  assert.equal(isAuthorizedRequest(request("Basic !!!"), auth), false);
  assert.equal(isAuthorizedRequest(request(basic("admin", "wrong")), auth), false);
  assert.equal(isAuthorizedRequest(request(basic("other", "secret")), auth), false);
});
