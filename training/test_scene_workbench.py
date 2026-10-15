import unittest
from scene_workbench import split_samples


class SplitTests(unittest.TestCase):
    def test_independent_videos_never_cross_splits(self):
        rows = [dict(label=label, batch=f'{label}-{batch}', seconds=i*.5) for label in ['yard','explore'] for batch in range(3) for i in range(10)]
        split, independent = split_samples(rows)
        self.assertTrue(independent)
        for batch in {r['batch'] for r in split}:
            self.assertEqual(len({r['split'] for r in split if r['batch']==batch}),1)

    def test_single_video_has_time_gap_and_honest_validation(self):
        rows = [dict(label='yard',batch='one',seconds=i*.5) for i in range(60)]
        split, independent = split_samples(rows)
        self.assertFalse(independent)
        train = [r['seconds'] for r in split if r['split']=='train']
        val = [r['seconds'] for r in split if r['split']=='val']
        self.assertGreaterEqual(min(val)-max(train),4)

    def test_too_short_video_is_rejected(self):
        with self.assertRaises(ValueError):
            split_samples([dict(label='yard',batch='one',seconds=i*.5) for i in range(3)])


if __name__ == '__main__':
    unittest.main()
