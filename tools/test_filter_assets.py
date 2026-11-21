"""Check conditional selection and include boundaries in bundled filters."""
import unittest
from filter_assets import assemble


class FilterAssetsTests(unittest.TestCase):
    def test_nested_conditions_and_relative_includes(self):
        files = {
            "filters/root.txt": b"!#if !cap_html_filtering\nfetch-fix\n!#if env_firefox\nfirefox-only\n!#else\n!#include child.txt\n!#endif\n!#else\nhtml-rewrite\n!#endif\nordinary\n",
            "filters/child.txt": b"!#if env_safari\nwebkit-fix\n!#else\nchromium-fix\n!#endif\n",
        }
        output, inputs = assemble("filters/root.txt", files.__getitem__)
        self.assertEqual(output, b"fetch-fix\nwebkit-fix\nordinary\n")
        self.assertEqual({item["path"] for item in inputs}, set(files))
        self.assertTrue(all(len(item["sha256"]) == 64 for item in inputs))

    def test_invalid_directives_and_recursive_includes_fail_closed(self):
        for data in [b"!#if unknown\nx\n!#endif", b"!#else", b"!#endif", b"!#if env_safari", b"!#if env_safari\n!#else\n!#else\n!#endif", b"!#include root.txt", b"!#include ../outside.txt"]:
            with self.subTest(data=data), self.assertRaises(ValueError):
                assemble("filters/root.txt", lambda _: data)


if __name__ == "__main__":
    unittest.main()
