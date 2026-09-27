import unittest
import import_events as compiler
import migrate_rows as migration
class ImportTests(unittest.TestCase):
 def test_literal_registrations_reject_expressions(self):
  self.assertEqual(compiler.strings('"Self fxDTSBrick" TAB "Player Player"'),['Self fxDTSBrick','Player Player'])
  with self.assertRaises(AssertionError):compiler.strings('"safe" @ evil()')
 def test_typed_vector_and_unresolved_datablock(self):
  self.assertEqual(migration.parse_value({'type':'vector','max_length':200},'0 0 10',{}),{'Vector':[0.,10.,-0.]})
  with self.assertRaises(AssertionError):migration.parse_value({'type':'datablock','class_name':'Sound'},'unmappedSound',{})
 def test_original_row_indices_and_unknown_data_are_preserved(self):
  c={'inputs':[{'name':'onRelay','targets':[['Self','fxDTSBrick']]}],'outputs':[{'name':'fireRelay','class_name':'fxDTSBrick','params':[]}]}
  data=migration.migrate(['+-EVENT\t2\t1\tonRelay\t0\tSelf\t\tfireRelay\t\t\t\t','+-EVENT\t3\t1\tcustomInput\t0\tSelf\t\tcustomOutput\t\t\t\t'],c,{})
  self.assertEqual(len(data['rows']),4);self.assertEqual(data['runnable'],1);self.assertEqual(data['preserved'],3);self.assertIsNone(data['rows'][2]['preserved']);self.assertIn('customInput',data['rows'][3]['preserved']['original'])
if __name__=='__main__':unittest.main()
