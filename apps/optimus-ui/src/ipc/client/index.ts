/** Renderer client (ADR-0090): the typed door over the frozen wire. */
export { createOptimusClient, type OptimusClient } from './client';
export { ChatSession, type SendOptions, type SendResult } from './chatSession';
export { createTurn, classifyTerminal, messageOf, type Turn } from './turn';
export { RuntimeObserver, type RuntimeEvent, type RuntimeEventInput } from './runtime';
export {
  IpcError,
  NoTransportError,
  TurnInFlightError,
  type TurnOutcome,
} from './types';
export type {
  ArtifactsApi,
  ApprovalsApi,
  BrowserApi,
  CampaignsApi,
  ConsentsApi,
  CronApi,
  CronAttempt,
  FsApi,
  GatewayApi,
  GatewayStatus,
  InboxMessage,
  JobsApi,
  MemoryApi,
  OutboxReceipt,
  PacksApi,
  PaletteCommand,
  ProjectsApi,
  ProviderKeyStatus,
  ProviderRow,
  ProvidersApi,
  SessionsApi,
  SessionConsent,
  SettingsApi,
  ShellApi,
  SkillsApi,
  SystemApi,
  TerminalResult,
} from './domains';
