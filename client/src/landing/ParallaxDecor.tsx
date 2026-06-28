/** Wireframe mountains + VHS grain (Blood Dragon parallax layers). */
export function ParallaxDecor() {
  return (
    <div className="landing__parallax" aria-hidden>
      <div className="landing__mountain landing__mountain--1" />
      <div className="landing__mountain landing__mountain--2" />
      <div className="landing__laser landing__laser--1" />
      <div className="landing__laser landing__laser--2" />
      <div className="landing__scanlines" />
      <div className="landing__grain" />
    </div>
  );
}
