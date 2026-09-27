import unittest
import import_events as compiler
class ImportTests(unittest.TestCase):
 def test_literal_registrations_reject_expressions(self):
  self.assertEqual(compiler.strings('"Self fxDTSBrick" TAB "Player Player"'),['Self fxDTSBrick','Player Player'])
  with self.assertRaises(AssertionError):compiler.strings('"safe" @ evil()')
if __name__=='__main__':unittest.main()
