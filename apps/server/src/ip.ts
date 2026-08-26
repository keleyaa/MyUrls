import { createHmac } from 'node:crypto';

import ipaddr from 'ipaddr.js';

export type IpAddress = ipaddr.IPv4 | ipaddr.IPv6;
export type Cidr = [IpAddress, number];

function stripBrackets(value: string): string {
  if (value.startsWith('[') && value.endsWith(']')) {
    return value.slice(1, -1);
  }
  return value;
}

export function parseIpAddress(value: string): IpAddress | undefined {
  const candidate = stripBrackets(value.trim());
  if (!ipaddr.isValid(candidate)) {
    return undefined;
  }
  try {
    return ipaddr.process(candidate);
  } catch {
    return undefined;
  }
}

export function canonicalizeIp(value: string): string | undefined {
  return parseIpAddress(value)?.toString();
}

export function parseCidr(value: string): Cidr {
  const [address, prefixLength] = ipaddr.parseCIDR(value);
  return [address, prefixLength];
}

export function isIpInCidrs(value: string, cidrs: readonly Cidr[]): boolean {
  const address = parseIpAddress(value);
  if (address === undefined) {
    return false;
  }
  return cidrs.some((cidr) => address.match(cidr));
}

function parseForwardedValues(value: string): string[] {
  const values: string[] = [];
  for (const element of value.split(',')) {
    for (const parameter of element.split(';')) {
      const [name, ...rest] = parameter.trim().split('=');
      if (name?.toLowerCase() !== 'for' || rest.length === 0) {
        continue;
      }
      const candidate = rest.join('=').trim().replace(/^"|"$/g, '');
      if (candidate !== 'unknown' && candidate !== '_hidden') {
        values.push(candidate);
      }
    }
  }
  return values;
}

function forwardedChain(headers: Record<string, string | undefined>): string[] {
  const xForwardedFor = headers['x-forwarded-for'];
  if (xForwardedFor !== undefined) {
    return xForwardedFor
      .split(',')
      .map((value) => value.trim())
      .filter((value) => value !== '');
  }
  const forwarded = headers.forwarded;
  return forwarded === undefined ? [] : parseForwardedValues(forwarded);
}

export function getClientIp(
  remoteAddress: string | undefined,
  headers: Record<string, string | undefined>,
  trustedProxyCidrs: readonly Cidr[],
): string {
  const direct = remoteAddress === undefined ? undefined : canonicalizeIp(remoteAddress);
  if (direct === undefined) {
    return 'unknown';
  }
  if (!isIpInCidrs(direct, trustedProxyCidrs)) {
    return direct;
  }

  let current = direct;
  for (const candidate of forwardedChain(headers).reverse()) {
    if (!isIpInCidrs(current, trustedProxyCidrs)) {
      break;
    }
    const canonical = canonicalizeIp(candidate);
    if (canonical === undefined) {
      break;
    }
    current = canonical;
  }
  return current;
}

export function fingerprintIp(secret: Buffer, clientIp: string): string {
  return createHmac('sha256', secret).update(clientIp, 'utf8').digest('hex');
}
