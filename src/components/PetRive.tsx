import { useEffect, useRef } from "react";
import { Alignment, Fit, Layout, useRive } from "@rive-app/react-canvas";
import type { Rive, StateMachineInput } from "@rive-app/canvas";
import { PetMood } from "../lib/types";
import flyingPetUrl from "../assets/rive/flying-pet.riv?url";

/** Swap to `../assets/rive/mote.riv?url` when a custom Mote file is ready. */
const MOTE_RIVE_SRC = flyingPetUrl;

const MOOD_ALIASES: Record<PetMood, string[]> = {
  idle: ["idle", "default", "loop"],
  glanceLeft: ["left", "glanceleft", "lookleft"],
  glanceRight: ["right", "glanceright", "lookright"],
  glanceUp: ["up", "glanceup", "lookup"],
  glanceDown: ["down", "glancedown", "lookdown"],
  curious: ["curious", "wonder", "interest", "hover"],
  drowsy: ["drowsy", "sleep", "sleepy", "tired"],
  peek: ["peek", "shy"],
  music: ["music", "dance", "happy", "joy"],
  focus: ["focus", "think", "work", "read"],
  talk: ["talk", "speak", "chat"],
  think: ["think", "thinking"],
  angry: ["angry", "mad", "rage"],
  laugh: ["laugh", "happy", "joy", "pressed"],
  cry: ["cry", "sad", "tear"],
  offended: ["offended", "sad", "hurt", "annoyed"],
};

function norm(s: string): string {
  return s.toLowerCase().replace(/[\s_-]+/g, "");
}

function applyMood(rive: Rive, mood: PetMood) {
  const aliases = MOOD_ALIASES[mood] ?? [mood];
  const sms = rive.stateMachineNames ?? [];
  if (sms.length === 0) {
    const anims = rive.animationNames ?? [];
    const hit =
      anims.find((a) => aliases.some((al) => norm(a).includes(al))) ?? anims[0];
    if (hit) rive.play(hit);
    return;
  }
  const sm = sms[0];
  const inputs: StateMachineInput[] = rive.stateMachineInputs(sm) ?? [];
  let fired = false;
  for (const input of inputs) {
    const n = norm(input.name);
    const match = aliases.some((al) => n.includes(al) || al.includes(n));
    if (!match) continue;
    if (typeof input.fire === "function" && typeof input.value !== "boolean" && typeof input.value !== "number") {
      input.fire();
      fired = true;
    } else if (typeof input.value === "boolean") {
      input.value = true;
      fired = true;
    } else if (typeof input.value === "number" && (n.includes("mood") || n.includes("state"))) {
      input.value = moodToIndex(mood);
      fired = true;
    }
  }
  for (const input of inputs) {
    if (typeof input.value !== "boolean") continue;
    const n = norm(input.name);
    const match = aliases.some((al) => n.includes(al) || al.includes(n));
    if (!match) input.value = false;
  }
  if (!fired) {
    rive.play(sm);
  }
}

function moodToIndex(mood: PetMood): number {
  const order: PetMood[] = [
    "idle",
    "curious",
    "talk",
    "think",
    "focus",
    "music",
    "laugh",
    "angry",
    "cry",
    "offended",
    "drowsy",
  ];
  const i = order.indexOf(mood);
  return i < 0 ? 0 : i;
}

function applyLook(rive: Rive, lookX: number, lookY: number) {
  const sms = rive.stateMachineNames ?? [];
  if (!sms.length) return;
  const inputs = rive.stateMachineInputs(sms[0]) ?? [];
  // App look is ~±8px; Rive often wants -1..1 or 0..100.
  const nx = Math.max(-1, Math.min(1, lookX / 8));
  const ny = Math.max(-1, Math.min(1, lookY / 8));
  for (const input of inputs) {
    if (typeof input.value !== "number") continue;
    const n = norm(input.name);
    if (n.includes("lookx") || n === "x" || n.includes("eyex") || n.includes("dirx")) {
      input.value = nx;
    } else if (n.includes("looky") || n === "y" || n.includes("eyey") || n.includes("diry")) {
      input.value = ny;
    }
  }
}

interface PetRiveProps {
  mood: PetMood;
  lookX: number;
  lookY: number;
  eyesTracking: boolean;
  onFailed: () => void;
}

export function PetRive({ mood, lookX, lookY, eyesTracking, onFailed }: PetRiveProps) {
  const failed = useRef(false);
  const { rive, RiveComponent } = useRive({
    src: MOTE_RIVE_SRC,
    autoplay: true,
    layout: new Layout({ fit: Fit.Contain, alignment: Alignment.Center }),
    shouldDisableRiveListeners: true,
    onLoadError: () => {
      if (failed.current) return;
      failed.current = true;
      console.warn("[mote] rive load failed — SVG fallback");
      onFailed();
    },
    onRiveReady: (instance) => {
      console.info(
        "[mote] rive ready sm=%o anim=%o",
        instance.stateMachineNames,
        instance.animationNames,
      );
      applyMood(instance, mood);
    },
  });

  useEffect(() => {
    if (!rive) return;
    applyMood(rive, mood);
  }, [rive, mood]);

  useEffect(() => {
    if (!rive || !eyesTracking) return;
    applyLook(rive, lookX, lookY);
  }, [rive, lookX, lookY, eyesTracking]);

  return <RiveComponent className="pet-rive-canvas" aria-hidden />;
}
