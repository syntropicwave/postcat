// postcat script sandbox prelude — builds the pm.* API around __pc_input
// (injected by the host) and __pc_send (host function for pm.sendRequest).
"use strict";

globalThis.__pc = (function () {
  const input = globalThis.__pc_input;
  const consoleLines = [];
  const tests = [];
  const varOps = [];
  const vars = Object.assign({}, input.vars);
  let nextRequest; // undefined = not set, "__stop__" = explicit null

  function fmt(args) {
    return args
      .map(function (a) {
        if (typeof a === "string") return a;
        try {
          return JSON.stringify(a);
        } catch (e) {
          return String(a);
        }
      })
      .join(" ");
  }

  globalThis.console = {
    log: function () {
      consoleLines.push(["log", fmt([].slice.call(arguments))]);
    },
    info: function () {
      consoleLines.push(["log", fmt([].slice.call(arguments))]);
    },
    warn: function () {
      consoleLines.push(["warn", fmt([].slice.call(arguments))]);
    },
    error: function () {
      consoleLines.push(["error", fmt([].slice.call(arguments))]);
    },
  };

  /* ---------------- mini-chai expect ---------------- */

  function deepEqual(a, b) {
    if (a === b) return true;
    if (typeof a !== typeof b) return false;
    if (a === null || b === null) return false;
    if (typeof a !== "object") return false;
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) if (!deepEqual(a[k], b[k])) return false;
    return true;
  }

  function show(v) {
    try {
      return JSON.stringify(v);
    } catch (e) {
      return String(v);
    }
  }

  function Expectation(value, negated) {
    this._value = value;
    this._negated = !!negated;
  }

  Expectation.prototype._assert = function (cond, msg) {
    const pass = this._negated ? !cond : cond;
    if (!pass)
      throw new Error(
        this._negated ? msg.replace("expected", "expected not") : msg,
      );
    return this;
  };

  // chainable no-op words
  [
    "to",
    "be",
    "been",
    "is",
    "that",
    "which",
    "and",
    "has",
    "have",
    "with",
    "at",
    "of",
    "same",
    "deep",
  ].forEach(function (word) {
    Object.defineProperty(Expectation.prototype, word, {
      get: function () {
        return this;
      },
    });
  });

  Object.defineProperty(Expectation.prototype, "not", {
    get: function () {
      return new Expectation(this._value, !this._negated);
    },
  });

  Expectation.prototype.equal =
    Expectation.prototype.equals =
    Expectation.prototype.eq =
      function (v) {
        return this._assert(
          this._value === v,
          "expected " + show(this._value) + " to equal " + show(v),
        );
      };
  Expectation.prototype.eql = function (v) {
    return this._assert(
      deepEqual(this._value, v),
      "expected " + show(this._value) + " to deeply equal " + show(v),
    );
  };
  Expectation.prototype.include =
    Expectation.prototype.contain =
    Expectation.prototype.includes =
      function (v) {
        let ok = false;
        if (typeof this._value === "string") ok = this._value.indexOf(v) !== -1;
        else if (Array.isArray(this._value))
          ok = this._value.some(function (x) {
            return deepEqual(x, v);
          });
        else if (this._value && typeof this._value === "object") {
          ok = Object.keys(v).every((k) => deepEqual(this._value[k], v[k]));
        }
        return this._assert(
          ok,
          "expected " + show(this._value) + " to include " + show(v),
        );
      };
  Expectation.prototype.property = function (name, value) {
    const has = this._value != null && name in Object(this._value);
    if (arguments.length > 1) {
      return this._assert(
        has && deepEqual(this._value[name], value),
        "expected " +
          show(this._value) +
          " to have property " +
          name +
          " = " +
          show(value),
      );
    }
    return this._assert(
      has,
      "expected " + show(this._value) + " to have property " + name,
    );
  };
  Expectation.prototype.lengthOf = function (n) {
    const len = this._value == null ? undefined : this._value.length;
    return this._assert(len === n, "expected length " + len + " to be " + n);
  };
  Expectation.prototype.above = Expectation.prototype.greaterThan = function (
    n,
  ) {
    return this._assert(
      this._value > n,
      "expected " + show(this._value) + " to be above " + n,
    );
  };
  Expectation.prototype.below = Expectation.prototype.lessThan = function (n) {
    return this._assert(
      this._value < n,
      "expected " + show(this._value) + " to be below " + n,
    );
  };
  Expectation.prototype.least = function (n) {
    return this._assert(
      this._value >= n,
      "expected " + show(this._value) + " to be at least " + n,
    );
  };
  Expectation.prototype.most = function (n) {
    return this._assert(
      this._value <= n,
      "expected " + show(this._value) + " to be at most " + n,
    );
  };
  Expectation.prototype.match = function (re) {
    return this._assert(
      re.test(this._value),
      "expected " + show(this._value) + " to match " + re,
    );
  };
  Expectation.prototype.a = Expectation.prototype.an = function (type) {
    const actual = Array.isArray(this._value)
      ? "array"
      : this._value === null
        ? "null"
        : typeof this._value;
    return this._assert(
      actual === type,
      "expected " +
        show(this._value) +
        " to be a " +
        type +
        " (got " +
        actual +
        ")",
    );
  };
  Expectation.prototype.oneOf = function (list) {
    return this._assert(
      list.some((x) => deepEqual(x, this._value)),
      "expected " + show(this._value) + " to be one of " + show(list),
    );
  };
  ["true", "false", "null", "undefined", "ok", "empty"].forEach(
    function (word) {
      Object.defineProperty(Expectation.prototype, word, {
        get: function () {
          if (word === "true")
            return this._assert(
              this._value === true,
              "expected " + show(this._value) + " to be true",
            );
          if (word === "false")
            return this._assert(
              this._value === false,
              "expected " + show(this._value) + " to be false",
            );
          if (word === "null")
            return this._assert(
              this._value === null,
              "expected " + show(this._value) + " to be null",
            );
          if (word === "undefined")
            return this._assert(
              this._value === undefined,
              "expected " + show(this._value) + " to be undefined",
            );
          if (word === "ok")
            return this._assert(
              !!this._value,
              "expected " + show(this._value) + " to be truthy",
            );
          const len =
            this._value == null
              ? 0
              : this._value.length !== undefined
                ? this._value.length
                : Object.keys(this._value).length;
          return this._assert(
            len === 0,
            "expected " + show(this._value) + " to be empty",
          );
        },
      });
    },
  );

  function expect(v) {
    return new Expectation(v, false);
  }

  /* ---------------- pm.request ---------------- */

  const req = input.request;
  function findHeader(k) {
    k = k.toLowerCase();
    return req.headers.find(function (h) {
      return h.key.toLowerCase() === k;
    });
  }
  const pmRequest = {
    get method() {
      return req.method;
    },
    set method(m) {
      req.method = String(m).toUpperCase();
    },
    get url() {
      return req.url;
    },
    set url(u) {
      req.url = String(u);
    },
    get body() {
      return req.body;
    },
    set body(b) {
      req.body = b;
    },
    headers: {
      add: function (h) {
        req.headers.push({ key: h.key, value: String(h.value), enabled: true });
      },
      upsert: function (h) {
        const e = findHeader(h.key);
        if (e) {
          e.value = String(h.value);
          e.enabled = true;
        } else
          req.headers.push({
            key: h.key,
            value: String(h.value),
            enabled: true,
          });
      },
      remove: function (k) {
        const i = req.headers.findIndex(function (h) {
          return h.key.toLowerCase() === k.toLowerCase();
        });
        if (i >= 0) req.headers.splice(i, 1);
      },
      get: function (k) {
        const h = findHeader(k);
        return h ? h.value : undefined;
      },
      has: function (k) {
        return !!findHeader(k);
      },
    },
  };

  /* ---------------- pm.response ---------------- */

  let pmResponse;
  if (input.response) {
    const r = input.response;
    function respHeader(k) {
      k = k.toLowerCase();
      const h = (r.headers || []).find(function (x) {
        return x[0].toLowerCase() === k;
      });
      return h ? h[1] : undefined;
    }
    pmResponse = {
      code: r.status,
      status: r.status_text,
      responseTime: r.duration_ms,
      responseSize: r.size,
      headers: {
        get: respHeader,
        has: function (k) {
          return respHeader(k) !== undefined;
        },
      },
      text: function () {
        return r.body_text || "";
      },
      json: function () {
        return JSON.parse(r.body_text || "null");
      },
      to: {
        have: {
          status: function (code) {
            if (typeof code === "number" && r.status !== code)
              throw new Error(
                "expected response to have status " +
                  code +
                  " but got " +
                  r.status,
              );
            if (typeof code === "string" && r.status_text !== code)
              throw new Error(
                "expected response status text " +
                  code +
                  " but got " +
                  r.status_text,
              );
          },
          header: function (k) {
            if (respHeader(k) === undefined)
              throw new Error("expected response to have header " + k);
          },
          jsonBody: function () {
            JSON.parse(r.body_text || "null");
          },
        },
        be: {
          get ok() {
            if (r.status < 200 || r.status >= 300)
              throw new Error("expected response to be ok but got " + r.status);
            return true;
          },
        },
      },
    };
  }

  /* ---------------- variable scopes ---------------- */

  function scopeApi(scope) {
    return {
      get: function (k) {
        return vars[k];
      },
      set: function (k, v) {
        vars[k] = String(v);
        varOps.push({ scope: scope, key: k, value: String(v) });
      },
      unset: function (k) {
        delete vars[k];
        varOps.push({ scope: scope, key: k, value: null });
      },
      has: function (k) {
        return Object.prototype.hasOwnProperty.call(vars, k);
      },
      replaceIn: function (s) {
        return String(s).replace(/\{\{([^}]+)\}\}/g, function (m, name) {
          const v = vars[name.trim()];
          return v === undefined ? m : v;
        });
      },
    };
  }

  /* ---------------- pm ---------------- */

  globalThis.pm = {
    request: pmRequest,
    response: pmResponse,
    environment: scopeApi("environment"),
    globals: scopeApi("global"),
    collectionVariables: scopeApi("collection"),
    variables: scopeApi("local"),
    expect: expect,
    test: function (name, fn) {
      try {
        fn();
        tests.push({ name: String(name), passed: true, error: null });
      } catch (e) {
        tests.push({
          name: String(name),
          passed: false,
          error: String((e && e.message) || e),
        });
      }
    },
    info: {
      iteration: input.iteration || 0,
      iterationCount: input.iteration_count || 1,
      requestName: input.request_name || "",
    },
    iterationData: {
      get: function (k) {
        return (input.data || {})[k];
      },
      toObject: function () {
        return input.data || {};
      },
    },
    execution: {
      setNextRequest: function (n) {
        nextRequest = n === null ? "__stop__" : String(n);
      },
      skipRequest: function () {
        nextRequest = "__skip__";
      },
    },
    sendRequest: function (reqDef, cb) {
      let spec;
      if (typeof reqDef === "string") {
        spec = {
          method: "GET",
          url: reqDef,
          headers: [],
          body: { kind: "none" },
        };
      } else {
        spec = {
          method: (reqDef.method || "GET").toUpperCase(),
          url: reqDef.url,
          headers: (reqDef.header || []).map(function (h) {
            return { key: h.key, value: String(h.value), enabled: true };
          }),
          body:
            reqDef.body && reqDef.body.mode === "raw"
              ? {
                  kind: "raw",
                  content_type: "application/json",
                  text: reqDef.body.raw,
                }
              : { kind: "none" },
        };
      }
      const result = JSON.parse(__pc_send(JSON.stringify(spec)));
      if (typeof cb === "function") {
        if (result.error) {
          cb(result.error, undefined);
        } else {
          cb(null, {
            code: result.status,
            status: result.status_text,
            headers: result.headers,
            text: function () {
              return result.body_text || "";
            },
            json: function () {
              return JSON.parse(result.body_text || "null");
            },
          });
        }
      }
      return result;
    },
  };

  return {
    result: function () {
      return JSON.stringify({
        tests: tests,
        console: consoleLines,
        request: req,
        varOps: varOps,
        nextRequest: nextRequest === undefined ? null : nextRequest,
      });
    },
  };
})();
