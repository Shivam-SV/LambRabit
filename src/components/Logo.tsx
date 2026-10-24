import { useEffect, useState } from "react";
import clsx from "clsx";
import { motionAllowed } from "../lib/motion";

const FIRST_PLAY_MS = 2_500;
const PLAY_MS = 3_400; // the whole ears → eyes → nose routine
const REST_MS = 3 * 60_000;

/** Plays the idle routine once shortly after mount, then every few minutes while motion is allowed. */
function useIdleRoutine(animated: boolean) {
  const [playing, setPlaying] = useState(false);
  useEffect(() => {
    if (!animated) return;
    let timer: number;
    const play = () => {
      if (motionAllowed() && document.visibilityState === "visible") setPlaying(true);
      timer = window.setTimeout(() => {
        setPlaying(false);
        timer = window.setTimeout(play, REST_MS);
      }, PLAY_MS);
    };
    timer = window.setTimeout(play, FIRST_PLAY_MS);
    return () => window.clearTimeout(timer);
  }, [animated]);
  return playing;
}

/** The lampRabit mark. The head colour follows the theme so it stays visible on dark surfaces. */
export default function Logo({ className, animated = false }: { className?: string; animated?: boolean }) {
  const playing = useIdleRoutine(animated);
  return (
    <svg
      viewBox="270 145 660 660"
      className={clsx(className, playing && "logo-play")}
      role="img"
      aria-label="lampRabit"
    >
      <path
        fill="var(--logo-head)"
        d="M376 365 L824 365 C865.974 365 900 399.026 900 441 L900 699 C900 740.974 865.974 775 824 775 L376 775 C334.026 775 300 740.974 300 699 L300 441 C300 399.026 334.026 365 376 365 Z"
      />
      {/* Ears sit on top of the head (same colour) so their base stays hidden while they move. */}
      <g className="logo-ear">
        <path
          fill="var(--logo-head)"
          d="M451 431 C420 389 379 328 375 263 C371 203 400 176 433 188 C482 206 512 283 519 385 L519 431 Z"
        />
        <path fill="#59d1a2" d="M457 377 C434 340 408 294 407 254 C406 222 418 208 433 216 C463 231 483 296 490 377 Z" />
      </g>
      <g className="logo-ear logo-ear-right">
        <path
          fill="var(--logo-head)"
          d="M749 431 L749 385 C756 283 786 206 835 188 C868 176 897 203 893 263 C889 328 848 389 817 431 Z"
        />
        <path fill="#59d1a2" d="M773 377 C780 296 800 231 830 216 C845 208 857 222 856 254 C855 294 829 340 806 377 Z" />
      </g>
      <path
        className="logo-eyes"
        d="M492 493 L427 558 L492 623 M708 493 L773 558 L708 623"
        fill="none"
        stroke="#59d1a2"
        strokeWidth="30"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        className="logo-nose"
        d="M580 647 L600 665 L620 647 M600 665 L600 697"
        fill="none"
        stroke="#ffffff"
        strokeWidth="13"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
