import { describe, expect, it } from "vitest";

import { formatBytes, formatEta, formatRate } from "./format";

describe("formatBytes", () => {
  it("keeps small values in bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("steps up units at 1024", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
  });
});

describe("formatRate", () => {
  it("reports zero and non-finite throughput as 0 B/s", () => {
    expect(formatRate(0)).toBe("0 B/s");
    expect(formatRate(-1)).toBe("0 B/s");
    expect(formatRate(Number.NaN)).toBe("0 B/s");
  });

  it("drops the space and the decimal for large values", () => {
    expect(formatRate(512)).toBe("512B/s");
    expect(formatRate(100 * 1024)).toBe("100K/s");
    expect(formatRate(1.5 * 1024 * 1024)).toBe("1.5M/s");
  });
});

describe("formatEta", () => {
  it("renders an empty string when the estimate is unknown", () => {
    expect(formatEta(null)).toBe("");
  });

  it("switches from seconds to minutes to hours", () => {
    expect(formatEta(59)).toBe("59s");
    expect(formatEta(60)).toBe("1m 0s");
    expect(formatEta(3661)).toBe("1h 1m");
  });
});
