import { useEffect } from "react";

/** Landing audio stub — native client has in-game audio. */
export function useLandingAudio() {
  useEffect(() => {}, []);
}

export function playLandingClick(): void {}

export function toggleLandingMute(): boolean {
  return false;
}
