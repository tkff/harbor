import { invoke, isTauri, Channel } from "@tauri-apps/api/core";
export { invoke, isTauri, Channel };
export type ForwardRule = {
  id: string;
  direction: "local" | "remote";
  bindAddress: string;
  bindPort: number;
  targetHost: string;
  targetPort: number;
  autoStart: boolean;
};
export type ForwardStatus = {
  state: "inactive" | "starting" | "active" | "error";
  message: string | null;
};
export type Profile = {
  id: string;
  name: string;
  host: string;
  port: number;
  folder: string;
  startupSnippet: string | null;
  sessionLogging: boolean;
  tags: string[];
  forwards: ForwardRule[];
  authMethod: "password" | "privateKey" | "certificate";
  identity: IdentityInfo | null;
};
export type IdentityInfo = {
  fingerprint: string;
  certificateId: string | null;
  principals: string[];
  validAfter: number | null;
  validBefore: number | null;
  caFingerprint: string | null;
};
export type HelloStatus = {
  supported: boolean;
  enabled: boolean;
  reason: string | null;
};
export type VaultStatus = {
  unlocked: boolean;
  vaultPath: string | null;
  profiles: Profile[];
  requiresKeyFile: boolean;
};
export type SshEvent =
  | { type: "data"; bytes: number[] }
  | { type: "hostKey"; host: string; port: number; fingerprint: string }
  | { type: "connected" }
  | {
      type: "authenticationFailed";
      username: string;
      method: Profile["authMethod"];
    }
  | {
      type: "forward";
      ruleId: string;
      state: ForwardStatus["state"];
      message: string | null;
    }
  | { type: "closed"; message: string };
export type Session = {
  id: string;
  profile: Profile;
  state: "connecting" | "connected" | "closed";
};
export const errorText = (e: unknown) =>
  e instanceof Error ? e.message : String(e);

export type Snippet = {
  id: string;
  name: string;
  group: string;
  tags: string[];
  body: string;
};

export type LoginCredentials = { username: string; password: string | null };
