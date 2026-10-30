"""Source recovery must preserve pinned identity and existing local files."""
import hashlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch
from urllib.parse import parse_qs

from map_ambience.sources import DownloadForm, ensure_master


class Sources(unittest.TestCase):
    def test_exact_download_form_ignores_other_forms(self):
        parser=DownloadForm('dl-form')
        parser.feed('<form id="other"><input name="id" value="wrong"></form>'
                    '<form id="dl-form" action="/modules/telecharger.php">'
                    '<input name="id" value="1157"><input name="format" value="wav"></form>')
        self.assertEqual(parser.action,'/modules/telecharger.php')
        self.assertEqual(parser.fields,{'id':'1157','format':'wav'})

    def test_existing_cache_identity_and_missing_original_diagnostic(self):
        with tempfile.TemporaryDirectory() as folder:
            cache=Path(folder)
            raw=b'test-only original bytes'
            row={'key':'test','input':'test.wav','input_sha256':hashlib.sha256(raw).hexdigest(),
                 'source':'https://lasonotheque.org/test.html'}
            with self.assertRaisesRegex(ValueError,'Missing selected original.*--download'):
                ensure_master(row,cache)
            (cache/'test.wav').write_bytes(raw)
            self.assertEqual(ensure_master(row,cache),cache/'test.wav')
            (cache/'test.wav').write_bytes(b'user changed this')
            with self.assertRaisesRegex(ValueError,'not overwritten'):
                ensure_master(row,cache,download=True)
            self.assertEqual((cache/'test.wav').read_bytes(),b'user changed this')

    def test_public_form_recovery_requires_exact_bytes_before_writing(self):
        raw=b'test-only pinned media'
        row={'key':'test','input':'test.wav','id':'1157','bytes':len(raw),
             'input_sha256':hashlib.sha256(raw).hexdigest(),
             'source':'https://lasonotheque.org/refrigerateur-s1157.html'}
        form=b'<form id="dl-form" action="/modules/telecharger.php"><input name="id" value="1157"><input name="format" value="wav"></form>'
        for downloaded,accepted in [(raw,True),(b'x'*len(raw),False)]:
            with self.subTest(accepted=accepted),tempfile.TemporaryDirectory() as folder:
                opener=MagicMock()
                opener.open.side_effect=[io.BytesIO(form),io.BytesIO(downloaded)]
                with patch('map_ambience.sources.build_opener',return_value=opener),patch('map_ambience.sources.time.sleep') as countdown:
                    if accepted:
                        target=ensure_master(row,Path(folder),True)
                        self.assertEqual(target.read_bytes(),raw)
                    else:
                        with self.assertRaisesRegex(ValueError,'no source adopted'):
                            ensure_master(row,Path(folder),True)
                        self.assertFalse((Path(folder)/'test.wav').exists())
                    countdown.assert_called_once_with(5)
                request=opener.open.call_args_list[0].args[0]
                self.assertEqual(parse_qs(request.data.decode()),{'id':['1157'],'format':['wav']})


if __name__=='__main__':unittest.main()
