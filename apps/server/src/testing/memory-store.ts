import type { LinkStore } from '../ports.js';

interface ExpiringValue {
  value: string;
  expiresAt: number;
}

export class MemoryLinkStore implements LinkStore {
  private readonly links = new Map<string, ExpiringValue>();
  private readonly counters = new Map<string, ExpiringValue>();
  private readonly risks = new Map<string, ExpiringValue>();

  async claim(code: string, targetUrl: string, ttlSeconds: number): Promise<boolean> {
    const key = `myurl:link:${code}`;
    if (this.read(this.links, key) !== undefined) {
      return false;
    }
    this.links.set(key, { value: targetUrl, expiresAt: Date.now() + ttlSeconds * 1000 });
    return true;
  }

  async lookup(code: string): Promise<string | undefined> {
    return this.read(this.links, `myurl:link:${code}`);
  }

  async incrementCreateCounters(
    fingerprint: string,
    utcDate: string,
  ): Promise<{ tenMinuteCount: number; dailyCount: number }> {
    const tenMinuteCount = this.increment(this.counters, `10m:${fingerprint}`, 600);
    const dailyCount = this.increment(this.counters, `1d:${utcDate}:${fingerprint}`, 172800);
    return { tenMinuteCount, dailyCount };
  }

  async getRiskScore(fingerprint: string): Promise<number> {
    const value = this.read(this.risks, fingerprint);
    return value === undefined ? 0 : Number(value);
  }

  async addRiskScore(fingerprint: string, points: number): Promise<number> {
    const value = this.read(this.risks, fingerprint);
    const score = (value === undefined ? 0 : Number(value)) + points;
    this.risks.set(fingerprint, { value: String(score), expiresAt: Date.now() + 600000 });
    return score;
  }

  async ping(): Promise<void> {}

  async close(): Promise<void> {}

  private increment(map: Map<string, ExpiringValue>, key: string, ttlSeconds: number): number {
    const value = this.read(map, key);
    const count = value === undefined ? 1 : Number(value) + 1;
    map.set(key, { value: String(count), expiresAt: Date.now() + ttlSeconds * 1000 });
    return count;
  }

  private read(map: Map<string, ExpiringValue>, key: string): string | undefined {
    const entry = map.get(key);
    if (entry === undefined) {
      return undefined;
    }
    if (entry.expiresAt <= Date.now()) {
      map.delete(key);
      return undefined;
    }
    return entry.value;
  }
}
