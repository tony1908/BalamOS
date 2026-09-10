import {
  CIRCLE_AGENT_WALLET_SKILL_INSTRUCTION,
  CIRCLE_AGENT_WALLET_SKILL_NAME,
} from "./circleAgentWallet";
import {
  HEDERA_MAINNET_SKILL_INSTRUCTION,
  HEDERA_MAINNET_SKILL_NAME,
} from "./hederaMainnet";

export interface SkillPlugin {
  name: string;
  title: string;
  description: string;
  tagline: string;
  icon: string;
  accent: string;
  accentEnd: string;
  addLabel: string;
  instruction: string;
}

export const SKILL_PLUGINS: SkillPlugin[] = [
  {
    name: CIRCLE_AGENT_WALLET_SKILL_NAME,
    title: "Circle Agent Wallets",
    description:
      "Container-backed only: opt in to ARC-TESTNET-first Circle CLI guidance.",
    tagline: "ARC-TESTNET-first agent wallet CLI",
    icon: "◎",
    accent: "#16C0AC",
    accentEnd: "#0E8C7E",
    addLabel: "Add Circle Agent Wallets",
    instruction: CIRCLE_AGENT_WALLET_SKILL_INSTRUCTION,
  },
  {
    name: HEDERA_MAINNET_SKILL_NAME,
    title: "Hedera Mainnet Read-only",
    description: "Ask the agent for public account balances and recent transactions.",
    tagline: "Read-only monitor + user-approved HBAR transfers",
    icon: "ℏ",
    accent: "#7C63E0",
    accentEnd: "#5B44B8",
    addLabel: "Add Hedera Mainnet skill",
    instruction: HEDERA_MAINNET_SKILL_INSTRUCTION,
  },
];
