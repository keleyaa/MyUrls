import type { CreateLinkInput, CreateLinkResponse } from '@myurl/contracts';

import { AUTO_CODE_LENGTH, LINK_TTL_SECONDS, MAX_CODE_ATTEMPTS, type AppConfig } from './config.js';
import {
  AliasUnavailableError,
  ChallengeInvalidError,
  ChallengeRequiredError,
  CodeGenerationExhaustedError,
  DependencyUnavailableError,
  MyUrlError,
  RateLimitedError,
} from './errors.js';
import { fingerprintIp } from './ip.js';
import { isReservedCode, normalizeAlias } from './domain/alias-policy.js';
import { evaluateRisk } from './domain/risk.js';
import { generateShortCode, isValidCode } from './domain/short-code.js';
import { normalizeTargetUrl } from './domain/url-policy.js';
import type { LinkStore, TurnstileVerifier } from './ports.js';

export interface CreateLinkContext {
  clientIp: string;
}

export interface ShortLinkServiceOptions {
  config: AppConfig;
  store: LinkStore;
  turnstile: TurnstileVerifier;
  now?: () => Date;
  generateCode?: () => string;
}

function utcDate(date: Date): string {
  return date.toISOString().slice(0, 10);
}

export class ShortLinkService {
  private readonly config: AppConfig;
  private readonly store: LinkStore;
  private readonly turnstile: TurnstileVerifier;
  private readonly now: () => Date;
  private readonly generateCode: () => string;

  constructor(options: ShortLinkServiceOptions) {
    this.config = options.config;
    this.store = options.store;
    this.turnstile = options.turnstile;
    this.now = options.now ?? (() => new Date());
    this.generateCode = options.generateCode ?? (() => generateShortCode());
  }

  async create(input: CreateLinkInput, context: CreateLinkContext): Promise<CreateLinkResponse> {
    const fingerprint = fingerprintIp(this.config.ipHashSecret, context.clientIp);
    const counts = await this.store.incrementCreateCounters(fingerprint, utcDate(this.now()));
    const riskScore = await this.store.getRiskScore(fingerprint);
    const decision = evaluateRisk({
      tenMinuteCount: counts.tenMinuteCount,
      dailyCount: counts.dailyCount,
      riskScore,
      limits: this.config.limits,
      challengeEnabled: this.config.turnstile.enabled,
      forceChallenge: this.config.testForceChallenge,
    });
    if (decision === 'block') {
      throw new RateLimitedError();
    }

    if (decision === 'challenge') {
      if (input.challengeToken === undefined || input.challengeToken === '') {
        throw new ChallengeRequiredError(this.config.turnstile.siteKey);
      }
      let verification: { valid: boolean };
      try {
        verification = await this.turnstile.verify(input.challengeToken);
      } catch {
        throw new DependencyUnavailableError();
      }
      if (!verification.valid) {
        await this.recordRisk(fingerprint, 3);
        throw new ChallengeInvalidError(this.config.turnstile.siteKey);
      }
    }

    let targetUrl: string;
    try {
      targetUrl = normalizeTargetUrl(input.url);
    } catch (error) {
      if (error instanceof MyUrlError && error.code === 'url_not_allowed') {
        await this.recordRisk(fingerprint, 1);
      }
      throw error;
    }

    let alias: string | undefined;
    if (input.alias !== undefined) {
      try {
        alias = normalizeAlias(input.alias);
      } catch (error) {
        if (error instanceof MyUrlError && error.code === 'alias_invalid') {
          await this.recordRisk(fingerprint, 1);
        }
        throw error;
      }
      if (isReservedCode(alias)) {
        await this.recordRisk(fingerprint, 1);
        throw new AliasUnavailableError();
      }
    }

    const code = alias ?? (await this.claimGeneratedCode(targetUrl));
    if (alias !== undefined) {
      const claimed = await this.store.claim(alias, targetUrl, LINK_TTL_SECONDS);
      if (!claimed) {
        await this.recordRisk(fingerprint, 1);
        throw new AliasUnavailableError();
      }
    }

    const expiresAt = new Date(this.now().getTime() + LINK_TTL_SECONDS * 1000).toISOString();
    return {
      code,
      shortUrl: `${this.config.publicBaseUrl}/${encodeURIComponent(code)}`,
      expiresAt,
    };
  }

  async resolve(code: string): Promise<string | undefined> {
    if (!isValidCode(code)) {
      return undefined;
    }
    return this.store.lookup(code);
  }

  private async claimGeneratedCode(targetUrl: string): Promise<string> {
    for (let attempt = 0; attempt < MAX_CODE_ATTEMPTS; attempt += 1) {
      const candidate = this.generateCode();
      if (candidate.length !== AUTO_CODE_LENGTH || isReservedCode(candidate)) {
        continue;
      }
      if (await this.store.claim(candidate, targetUrl, LINK_TTL_SECONDS)) {
        return candidate;
      }
    }
    throw new CodeGenerationExhaustedError();
  }

  private async recordRisk(fingerprint: string, points: number): Promise<void> {
    try {
      await this.store.addRiskScore(fingerprint, points);
    } catch {
      throw new DependencyUnavailableError();
    }
  }
}

export function createDefaultService(options: ShortLinkServiceOptions): ShortLinkService {
  return new ShortLinkService(options);
}
