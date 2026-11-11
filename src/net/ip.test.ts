import { describe, expect, it } from "vitest";

import {
  describeSubnet,
  formatAddress,
  parseAddress,
  splitCidr,
  type SubnetReport,
} from "./ip";

function report(input: string): SubnetReport {
  const result = describeSubnet(input);
  if (typeof result === "string") {
    throw new Error(`expected a report for ${input}, got ${result}`);
  }
  return result;
}

describe("parseAddress", () => {
  it("parses IPv4 octets", () => {
    expect(parseAddress("192.168.0.1")).toEqual({ value: 0xc0a80001n, version: 4 });
  });

  it("rejects malformed IPv4", () => {
    for (const value of ["256.0.0.1", "1.2.3", "1.2.3.4.5", "1.2.3.x"]) {
      expect(parseAddress(value)).toBeNull();
    }
  });

  it("parses IPv6 with a compressed run and an embedded IPv4 tail", () => {
    expect(parseAddress("2001:db8::1")).toEqual({
      value: 0x20010db8000000000000000000000001n,
      version: 6,
    });
    expect(parseAddress("::ffff:192.168.0.1")).toEqual({
      value: 0xffffc0a80001n,
      version: 6,
    });
  });

  it("rejects IPv6 with more than one ::", () => {
    expect(parseAddress("2001::db8::1")).toBeNull();
    expect(parseAddress("2001:db8:0:0:0:0:0:1:1")).toBeNull();
  });
});

describe("formatAddress", () => {
  it("compresses the longest zero run per RFC 5952", () => {
    expect(formatAddress(0x20010db8000000000000000000000001n, 6)).toBe(
      "2001:db8::1",
    );
    expect(formatAddress(0xffffc0a80001n, 6)).toBe("::ffff:c0a8:1");
    expect(formatAddress(0x20010db8000000010000000000000001n, 6)).toBe(
      "2001:db8:0:1::1",
    );
  });

  it("does not compress a single zero group", () => {
    expect(formatAddress(0x20010db8000000010001000100010001n, 6)).toBe(
      "2001:db8:0:1:1:1:1:1",
    );
  });
});

describe("splitCidr", () => {
  it("splits address and prefix, tolerating whitespace", () => {
    expect(splitCidr(" 10.0.0.1 / 8 ")).toEqual({ address: "10.0.0.1", prefix: 8 });
    expect(splitCidr("10.0.0.1")).toEqual({ address: "10.0.0.1", prefix: null });
  });
});

describe("describeSubnet", () => {
  it("describes an IPv4 /24", () => {
    const result = report("192.168.1.0/24");
    expect(result).toMatchObject({
      version: 4,
      prefix: 24,
      address: "192.168.1.0",
      network: "192.168.1.0",
      last: "192.168.1.255",
      firstUsable: "192.168.1.1",
      lastUsable: "192.168.1.254",
      netmask: "255.255.255.0",
      wildcard: "0.0.0.255",
      hostCount: "256",
      usableCount: "254",
      scope: "private",
    });
  });

  it("counts hosts for a /0 without special casing", () => {
    const result = report("0.0.0.0/0");
    expect(result.network).toBe("0.0.0.0");
    expect(result.last).toBe("255.255.255.255");
    expect(result.hostCount).toBe("4294967296");
    expect(result.usableCount).toBe("4294967294");
  });

  it("treats a host route as having no usable hosts", () => {
    const result = report("2001:db8::1/128");
    expect(result.version).toBe(6);
    expect(result.firstUsable).toBe("2001:db8::1");
    expect(result.lastUsable).toBe("2001:db8::1");
    expect(result.usableCount).toBe("0");
  });

  it("applies the host prefix when none is given", () => {
    expect(report("10.1.2.3").prefix).toBe(32);
    expect(report("2001:db8::1").prefix).toBe(128);
  });

  it("classifies scopes", () => {
    expect(report("127.0.0.1/32").scope).toBe("loopback");
    expect(report("169.254.1.1/32").scope).toBe("linkLocal");
    expect(report("224.0.0.1/32").scope).toBe("multicast");
    expect(report("240.0.0.1/32").scope).toBe("reserved");
    expect(report("100.64.0.1/32").scope).toBe("private");
    expect(report("8.8.8.8/32").scope).toBe("public");
    expect(report("fe80::1/128").scope).toBe("linkLocal");
    expect(report("fd00::1/128").scope).toBe("private");
  });

  it("classifies an IPv4-mapped address by its IPv4 part", () => {
    expect(report("::ffff:192.168.0.1/128").scope).toBe("private");
  });

  it("reports invalid input instead of guessing", () => {
    expect(describeSubnet("not-an-ip")).toBe("invalid-address");
    expect(describeSubnet("10.0.0.0/33")).toBe("invalid-prefix");
    expect(describeSubnet("2001:db8::/129")).toBe("invalid-prefix");
  });
});
