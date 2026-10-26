import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from ffmpeg_cmd import FORBIDDEN_A_S_CODECS, assert_stream_copy, build_ffmpeg_args  # noqa: E402
from params import PipelineParams  # noqa: E402


class FfmpegCopyTests(unittest.TestCase):
    def test_full_mode_copies_audio_and_subs(self):
        args = build_ffmpeg_args(
            "ffmpeg",
            Path("in.mkv"),
            Path("out.mkv"),
            PipelineParams(),
            mode="full",
        )
        assert_stream_copy(args)
        self.assertEqual(args[args.index("-c:a") + 1], "copy")
        self.assertEqual(args[args.index("-c:s") + 1], "copy")
        self.assertEqual(args[args.index("-c:t") + 1], "copy")
        self.assertEqual(args[args.index("-c:v") + 1], "hevc_nvenc")
        self.assertIn("pipe:0", args)
        self.assertIn("in.mkv", args)
        joined = " ".join(args)
        for bad in FORBIDDEN_A_S_CODECS:
            # codec values only — "ass" must not be selected as -c:s
            self.assertNotIn(f"-c:a {bad}", joined)
            self.assertNotIn(f"-c:s {bad}", joined)

    def test_remux_still_copies_audio_subs(self):
        args = build_ffmpeg_args(
            "ffmpeg",
            Path("in.mkv"),
            Path("out.mkv"),
            PipelineParams(),
            mode="remux",
        )
        assert_stream_copy(args)
        self.assertEqual(args[args.index("-c:v") + 1], "copy")
        self.assertEqual(args[args.index("-c:a") + 1], "copy")
        self.assertEqual(args[args.index("-c:s") + 1], "copy")

    def test_maps_original_file_for_audio_subs(self):
        args = build_ffmpeg_args(
            "ffmpeg", Path(r"C:\anime\ep.mkv"), Path("out.mkv"), PipelineParams(), mode="full"
        )
        # second -i is the original, then 1:a? 1:s?
        i_flags = [i for i, a in enumerate(args) if a == "-i"]
        self.assertGreaterEqual(len(i_flags), 2)
        self.assertEqual(args[i_flags[1] + 1], r"C:\anime\ep.mkv")
        self.assertIn("1:a?", args)
        self.assertIn("1:s?", args)

    def test_refuses_audio_transcode_params(self):
        with self.assertRaises(ValueError):
            PipelineParams(copy_audio=False).validate()

    def test_assert_stream_copy_catches_aac(self):
        with self.assertRaises(AssertionError):
            assert_stream_copy(["ffmpeg", "-c:a", "aac", "-c:s", "copy"])


if __name__ == "__main__":
    unittest.main()
