export interface LinkStore {
  claim(code: string, targetUrl: string, ttlSeconds: number): Promise<boolean>;
  lookup(code: string): Promise<string | undefined>;
  incrementCreateCounters(
    fingerprint: string,
    utcDate: string,
  ): Promise<{ tenMinuteCount: number; dailyCount: number }>;
  getRiskScore(fingerprint: string): Promise<number>;
  addRiskScore(fingerprint: string, points: number): Promise<number>;
  ping(): Promise<void>;
  close(): Promise<void>;
}

export interface TurnstileVerifier {
  verify(token: string): Promise<{ valid: boolean }>;
}
