import {
  CIRCLE_AGENT_WALLET_SKILL_INSTRUCTION,
  CIRCLE_AGENT_WALLET_SKILL_NAME,
} from "./circleAgentWallet";
import {
  HEDERA_MAINNET_SKILL_INSTRUCTION,
  HEDERA_MAINNET_SKILL_NAME,
} from "./hederaMainnet";
import {
  THE_GRAPH_SKILL_INSTRUCTION,
  THE_GRAPH_SKILL_NAME,
} from "./theGraph";
import { X402_SKILL_INSTRUCTION, X402_SKILL_NAME } from "./x402";

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
  {
    name: THE_GRAPH_SKILL_NAME,
    title: "The Graph",
    tagline: "Query Subgraphs for balances, positions, and prices",
    description:
      "Ask the agent \"what's in my wallet\" — it reads on-chain data from The Graph's Subgraphs. Read-only.",
    addLabel: "Add The Graph reader",
    instruction: THE_GRAPH_SKILL_INSTRUCTION,
    icon: "⬡",
    accent: "#3E7BE6",
    accentEnd: "#2456B0",
  },
  {
    name: X402_SKILL_NAME,
    title: "x402 Payments",
    tagline: "Inspect and pay for x402-gated services",
    description:
      "The agent discovers the price of a pay-per-use (HTTP 402) service. Paying is added later behind governance.",
    addLabel: "Add x402 Payments",
    instruction: X402_SKILL_INSTRUCTION,
    icon: "⚡",
    accent: "#E0913A",
    accentEnd: "#B86A18",
  },
];
