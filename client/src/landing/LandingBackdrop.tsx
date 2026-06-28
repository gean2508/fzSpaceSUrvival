/** PS1-era banded sky + CRT scanlines (landing only). */
export function LandingBackdrop() {
  return (
    <div className="landing__backdrop" aria-hidden>
      <div className="landing__sky" />
      <div className="landing__dither" />
      <div className="landing__scanlines" />
    </div>
  );
}
