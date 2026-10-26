import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from params import PipelineParams  # noqa: E402
from rife_plan import rife_plan  # noqa: E402
from vpy_template import render_vpy  # noqa: E402


class VpyOrderTests(unittest.TestCase):
    def test_rife_before_esrgan_and_no_resize_before_rife(self):
        text = render_vpy(Path("ep.mkv"), PipelineParams(), rife_plan(23.976), Path("cache"))
        rife_at = text.index("clip = RIFE(")
        esrgan_at = text.index("clip = RealESRGAN(")
        self.assertLess(rife_at, esrgan_at)
        graph = text[text.index("clip = _to_rgbh") : rife_at]
        self.assertNotIn("Spline36", graph)
        self.assertNotIn("clip = RealESRGAN", graph)
        self.assertIn("Backend.TRT", text)
        self.assertIn("fp16=FP16", text)
        self.assertIn("FP16 = True", text)

    def test_24fps_plan_in_script(self):
        plan = rife_plan(24.0)
        self.assertEqual(plan.kind, "multiply_then_thin")
        self.assertEqual(plan.multi, 5)
        self.assertEqual(plan.cycle, 2)
        text = render_vpy(Path("ep.mkv"), PipelineParams(), plan, None)
        self.assertIn('"multi": 5', text)
        self.assertIn('"cycle": 2', text)

    def test_skips_live_and_ultra(self):
        text = render_vpy(Path("ep.mkv"), PipelineParams(), rife_plan(30), None)
        body = text.split("clip = _load", 1)[-1].lower()
        self.assertNotIn("ultra", body)
        self.assertNotIn("realtime", body)
        self.assertNotIn("live_rife", body)


if __name__ == "__main__":
    unittest.main()
