datablock StaticShapeData(ShortRifleRaycastTracer)
{
   shapeFile = "./line.dts";
};

function eulerToAxis(%euler)
{
   return getWords(MatrixCreateFromEuler(VectorScale(%euler, $pi / 180)), 3, 6);
}

function ShortRifle::drawTracer(%pos1, %pos2)
{
   %shape = new StaticShape() {datablock = ShortRifleRaycastTracer;};
   MissionCleanup.add(%shape);
   %shape.setTransform(vectorScale(vectorAdd(%pos1, %pos2), 0.5) SPC eulerToAxis("0 90 0"));
   %shape.setScale("1 1" SPC VectorLen(VectorSub(%pos2, %pos1)));
   return %shape;
}
