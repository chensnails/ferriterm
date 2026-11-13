/**
 * IPv4 / IPv6 address maths for the subnet calculator. Everything stays in
 * BigInt so a /0 or a 128-bit prefix needs no special casing.
 */

export type IpVersion = 4 | 6;

export type AddressScope =
  | "private"
  | "public"
  | "loopback"
  | "linkLocal"
  | "multicast"
  | "reserved";

export interface SubnetReport {
  version: IpVersion;
  prefix: number;
  address: string;
  network: string;
  last: string;
  firstUsable: string;
  lastUsable: string;
  netmask: string;
  wildcard: string;
  hostCount: string;
  usableCount: string;
  scope: AddressScope;
  binaryAddress: string;
  binaryNetwork: string;
  binaryMask: string;
}

const V4_MAX = (1n << 32n) - 1n;
const V6_MAX = (1n << 128n) - 1n;

function parseIpv4(value: string): bigint | null {
  const parts = value.split(".");
  if (parts.length !== 4) return null;
  let total = 0n;
  for (const part of parts) {
    if (!/^\d{1,3}$/.test(part)) return null;
    const octet = Number(part);
    if (octet > 255) return null;
    total = (total << 8n) | BigInt(octet);
  }
  return total;
}

function parseIpv6(value: string): bigint | null {
  let text = value.trim().toLowerCase();
  if (text.startsWith("[") && text.endsWith("]")) text = text.slice(1, -1);
  if (!text.includes(":")) return null;

  // A trailing dotted quad (::ffff:192.168.0.1) counts as two 16-bit groups.
  const v4Tail = text.match(/^(.*:)(\d{1,3}(?:\.\d{1,3}){3})$/);
  if (v4Tail) {
    const tail = parseIpv4(v4Tail[2]);
    if (tail === null) return null;
    text = `${v4Tail[1]}${(tail >> 16n).toString(16)}:${(tail & 0xffffn).toString(16)}`;
  }

  const halves = text.split("::");
  if (halves.length > 2) return null;
  const head = halves[0] ? halves[0].split(":") : [];
  const tail = halves.length === 2 && halves[1] ? halves[1].split(":") : [];
  const missing = 8 - head.length - tail.length;
  if (halves.length === 1 ? head.length !== 8 : missing < 1) return null;

  const groups = [...head, ...Array<string>(halves.length === 2 ? missing : 0).fill("0"), ...tail];
  let total = 0n;
  for (const group of groups) {
    if (!/^[0-9a-f]{1,4}$/.test(group)) return null;
    total = (total << 16n) | BigInt(`0x${group}`);
  }
  return total;
}

export function parseAddress(input: string): { value: bigint; version: IpVersion } | null {
  const text = input.trim();
  if (!text) return null;
  if (text.includes(":")) {
    const value = parseIpv6(text);
    return value === null ? null : { value, version: 6 };
  }
  const value = parseIpv4(text);
  return value === null ? null : { value, version: 4 };
}

function formatIpv4(value: bigint): string {
  const octets: string[] = [];
  let rest = value;
  for (let index = 0; index < 4; index += 1) {
    octets.unshift((rest & 0xffn).toString());
    rest >>= 8n;
  }
  return octets.join(".");
}

function formatIpv6(value: bigint): string {
  const groups: number[] = [];
  for (let index = 7; index >= 0; index -= 1) {
    groups.push(Number((value >> BigInt(index * 16)) & 0xffffn));
  }
  // Compress the longest run of zero groups (RFC 5952).
  let bestStart = -1;
  let bestLength = 0;
  let start = -1;
  groups.forEach((group, index) => {
    if (group === 0 && start === -1) start = index;
    if (group !== 0 || index === groups.length - 1) {
      const end = group === 0 ? index + 1 : index;
      const length = end - start;
      if (start >= 0 && length > bestLength) {
        bestStart = start;
        bestLength = length;
      }
      start = -1;
    }
  });
  const hex = groups.map((group) => group.toString(16));
  if (bestStart < 0 || bestLength < 2) return hex.join(":");
  const head = hex.slice(0, bestStart).join(":");
  const tail = hex.slice(bestStart + bestLength).join(":");
  return `${head}::${tail}`;
}

export function formatAddress(value: bigint, version: IpVersion): string {
  return version === 4 ? formatIpv4(value) : formatIpv6(value);
}

function binaryOf(value: bigint, bits: number): string {
  const width = bits === 32 ? 8 : 16;
  const parts: string[] = [];
  for (let shift = bits - width; shift >= 0; shift -= width) {
    const group = (value >> BigInt(shift)) & ((1n << BigInt(width)) - 1n);
    parts.push(group.toString(2).padStart(width, "0"));
  }
  return parts.join(" ");
}

function maskOf(prefix: number, bits: number): bigint {
  const max = bits === 32 ? V4_MAX : V6_MAX;
  if (prefix <= 0) return 0n;
  if (prefix >= bits) return max;
  const shift = BigInt(bits - prefix);
  return max ^ ((1n << shift) - 1n);
}

function scopeOf(value: bigint, version: IpVersion): AddressScope {
  if (version === 6) {
    if (value === 0n) return "reserved";
    // IPv4-mapped addresses (::ffff:a.b.c.d) describe an IPv4 host.
    if (value >> 32n === 0xffffn) return scopeOf(value & 0xffffffffn, 4);
    if (value === 1n) return "loopback";
    if (value >> 120n === 0xffn) return "multicast";
    if (value >> 118n === 0x3fan) return "linkLocal"; // fe80::/10
    if (value >> 121n === 0x7en) return "private"; // fc00::/7
    return "public";
  }
  const first = Number(value >> 24n);
  const second = Number((value >> 16n) & 0xffn);
  if (first === 0) return "reserved";
  if (first === 10) return "private";
  if (first === 127) return "loopback";
  if (first === 172 && second >= 16 && second <= 31) return "private";
  if (first === 192 && second === 168) return "private";
  if (first === 169 && second === 254) return "linkLocal";
  if (first === 100 && second >= 64 && second <= 127) return "private";
  if (first >= 224 && first <= 239) return "multicast";
  if (first >= 240) return "reserved";
  return "public";
}

export function splitCidr(input: string): { address: string; prefix: number | null } {
  const text = input.trim();
  const slash = text.lastIndexOf("/");
  if (slash < 0) return { address: text, prefix: null };
  const raw = text.slice(slash + 1).trim();
  const prefix = /^\d{1,3}$/.test(raw) ? Number(raw) : null;
  return { address: text.slice(0, slash).trim(), prefix };
}

export function describeSubnet(input: string): SubnetReport | string {
  const { address: rawAddress, prefix: rawPrefix } = splitCidr(input);
  const parsed = parseAddress(rawAddress);
  if (!parsed) return "invalid-address";
  const bits = parsed.version === 4 ? 32 : 128;
  const prefix = rawPrefix ?? (parsed.version === 4 ? 32 : 128);
  if (!Number.isInteger(prefix) || prefix < 0 || prefix > bits) return "invalid-prefix";

  const mask = maskOf(prefix, bits);
  const network = parsed.value & mask;
  const last = network | (mask ^ (bits === 32 ? V4_MAX : V6_MAX));
  const hostBits = BigInt(bits - prefix);
  const hostCount = 1n << hostBits;
  const usable = prefix >= bits - 1 ? 0n : hostCount - 2n;
  const firstUsable = prefix >= bits - 1 ? network : network + 1n;
  const lastUsable = prefix >= bits - 1 ? network : last - 1n;

  return {
    version: parsed.version,
    prefix,
    address: formatAddress(parsed.value, parsed.version),
    network: formatAddress(network, parsed.version),
    last: formatAddress(last, parsed.version),
    firstUsable: formatAddress(firstUsable, parsed.version),
    lastUsable: formatAddress(lastUsable, parsed.version),
    netmask:
      parsed.version === 4
        ? formatIpv4(mask)
        : `${formatAddress(mask, 6)} /${prefix}`,
    wildcard: formatAddress(mask ^ (bits === 32 ? V4_MAX : V6_MAX), parsed.version),
    hostCount: hostCount.toString(),
    usableCount: usable.toString(),
    scope: scopeOf(parsed.value, parsed.version),
    binaryAddress: binaryOf(parsed.value, bits),
    binaryNetwork: binaryOf(network, bits),
    binaryMask: binaryOf(mask, bits),
  };
}

/* ---------- helpers for the richer IP calculator modules ---------- */

function countLeadingOnes(value: bigint, bits: number): number {
  let count = 0;
  for (let shift = bits - 1; shift >= 0; shift -= 1) {
    if ((value >> BigInt(shift)) & 1n) count += 1;
    else break;
  }
  return count;
}

/** IPv4 dotted-decimal netmask for a prefix length, or null if out of range. */
export function netmaskForPrefix(prefix: number): string | null {
  if (!Number.isInteger(prefix) || prefix < 0 || prefix > 32) return null;
  return formatIpv4(maskOf(prefix, 32));
}

/** IPv4 hexadecimal netmask (per-octet, uppercase) for a prefix length. */
export function hexMaskForPrefix(prefix: number): string | null {
  if (!Number.isInteger(prefix) || prefix < 0 || prefix > 32) return null;
  const mask = maskOf(prefix, 32);
  return [24, 16, 8, 0]
    .map((shift) => ((mask >> BigInt(shift)) & 0xffn).toString(16).padStart(2, "0").toUpperCase())
    .join(".");
}

/** Prefix length for a contiguous dotted-decimal netmask, or null if invalid. */
export function prefixForNetmask(mask: string): number | null {
  const parsed = parseAddress(mask);
  if (!parsed || parsed.version !== 4) return null;
  const leading = countLeadingOnes(parsed.value, 32);
  if (maskOf(leading, 32) !== parsed.value) return null;
  return leading;
}

/** Split an IPv4 address into four octets, or null if not a valid IPv4 address. */
export function ipv4Octets(ip: string): number[] | null {
  const parsed = parseAddress(ip);
  if (!parsed || parsed.version !== 4) return null;
  const value = parsed.value;
  return [
    Number((value >> 24n) & 0xffn),
    Number((value >> 16n) & 0xffn),
    Number((value >> 8n) & 0xffn),
    Number(value & 0xffn),
  ];
}

/** Dotted-binary form of an IPv4 address, or null when invalid. */
export function ipv4ToBinary(ip: string): string | null {
  const octets = ipv4Octets(ip);
  if (!octets) return null;
  return octets.map((octet) => octet.toString(2).padStart(8, "0")).join(".");
}

/** Dotted-hexadecimal form of an IPv4 address, or null when invalid. */
export function ipv4ToHex(ip: string): string | null {
  const octets = ipv4Octets(ip);
  if (!octets) return null;
  return octets.map((octet) => octet.toString(16).padStart(2, "0").toUpperCase()).join(".");
}

/** IPv4 address from a 32-bit binary string (dots optional), or null. */
export function binaryToIpv4(binary: string): string | null {
  const bits = binary.replace(/[^01]/g, "");
  if (bits.length !== 32) return null;
  return formatIpv4(BigInt(`0b${bits}`));
}

/** IPv4 address from an 8-digit hex string (dots optional), or null. */
export function hexToIpv4(hex: string): string | null {
  const digits = hex.replace(/\./g, "").replace(/[^0-9a-fA-F]/g, "");
  if (digits.length !== 8) return null;
  return formatIpv4(BigInt(`0x${digits.toLowerCase()}`));
}

/** Smallest prefix whose block holds at least `hosts` addresses, or null. */
export function prefixForHostCount(hosts: number): number | null {
  if (!Number.isInteger(hosts) || hosts < 1) return null;
  if (hosts > 2 ** 32) return null;
  const needed = Math.ceil(Math.log2(hosts));
  return Math.max(0, Math.min(32, 32 - needed));
}

/** Wildcard (inverse) mask for a dotted-decimal netmask, or null if invalid. */
export function wildcardForMask(mask: string): string | null {
  const parsed = parseAddress(mask);
  if (!parsed || parsed.version !== 4) return null;
  return formatIpv4(V4_MAX ^ parsed.value);
}

/** Network / node / broadcast for an IPv4 address and its dotted netmask. */
export function networkNodeBroadcast(
  ip: string,
  mask: string,
): { network: string; node: string; broadcast: string } | null {
  const parsedIp = parseAddress(ip);
  const parsedMask = parseAddress(mask);
  if (!parsedIp || parsedIp.version !== 4 || !parsedMask || parsedMask.version !== 4) {
    return null;
  }
  const maskValue = parsedMask.value;
  const network = parsedIp.value & maskValue;
  const wildcard = V4_MAX ^ maskValue;
  return {
    network: formatIpv4(network),
    node: formatIpv4(parsedIp.value & wildcard),
    broadcast: formatIpv4(network | wildcard),
  };
}
