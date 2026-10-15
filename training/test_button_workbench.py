import copy
from pathlib import Path
import tempfile
import unittest
from button_workbench import validate_dataset


class DatasetTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root/'images').mkdir()
        self.data = dict(classes=[dict(id='challenge',name='挑战',anchor=[.5,.5])], samples=[])
        for split in ['train','val']:
            (self.root/'images'/f'{split}.png').write_bytes(split.encode())
            self.data['samples'].append(dict(id=split,split=split,batch=split,reviewed=True,width=1920,height=1080,
                boxes=[dict(label='challenge',bounds=[1600,800,1800,1000])]))

    def tearDown(self):
        self.temp.cleanup()

    def test_independent_batches_and_negative_samples(self):
        negative = copy.deepcopy(self.data['samples'][0]);negative.update(id='negative',boxes=[])
        (self.root/'images/negative.png').write_bytes(b'negative')
        self.data['samples'].append(negative)
        rows,counts=validate_dataset(self.data,self.root)
        self.assertEqual(len(rows),3)
        self.assertEqual(counts['challenge'],dict(train=1,val=1))

    def test_allows_same_capture_batch_across_splits(self):
        self.data['samples'][1]['batch']='train'
        rows, counts = validate_dataset(self.data,self.root)
        self.assertEqual([row['split'] for row in rows], ['train', 'val'])
        self.assertEqual(counts['challenge'], dict(train=1, val=1))

    def test_rejects_duplicate_image_across_splits(self):
        (self.root/'images/val.png').write_bytes(b'train')
        with self.assertRaisesRegex(ValueError,'相同图片'):validate_dataset(self.data,self.root)

    def test_unreviewed_is_not_a_negative(self):
        self.data['samples'][1]['reviewed']=False
        with self.assertRaisesRegex(ValueError,'缺少'):validate_dataset(self.data,self.root)

    def test_rejects_out_of_bounds_annotation(self):
        self.data['samples'][0]['boxes'][0]['bounds'][2]=2000
        with self.assertRaisesRegex(ValueError,'边界'):validate_dataset(self.data,self.root)


if __name__=='__main__':unittest.main()
