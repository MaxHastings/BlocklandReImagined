function ShortRifleImage::fireRaycast(%this, %obj, %slot)
{
	while(%hits <= %this.raycastRicochets)
	{
	%aimVec = VectorSub(%aimVec, VectorScale(%normal, VectorDot(%aimVec, %normal) * 2));
	}
}

function ShortRifleImage::onHitObject(%this, %obj, %slot, %col, %pos, %normal, %shotVec, %crit, %hits)
{
   if(%col.getType() & ($TypeMasks::PlayerObjectType | $TypeMasks::VehicleObjectType))
      %this.onRaycastDamage(%obj, %slot, %col, %pos, %normal, %shotVec, %crit, %hits);
   serverplay3d(%this.raycastExplosionPlayerSound, %pos);
}

function ShortRifleImage::onRaycastDamage(%this, %obj, %slot, %col, %pos, %normal, %shotVec, %crit, %hits)
{
   %damageType = %this.raycastDirectDamageType;
   %directDamage = %this.raycastDirectDamage;
   if(%obj == %col)
      %directDamage *= 0.25;
   else
      %directDamage += %hits * 15;
   %directDamage = mClampF(%directDamage, -100, 100);

   if(%crit)
   {
      %damageType = %this.raycastCritDirectDamageType;
      if(%crit > 1)
      {
         %directDamage = %directDamage * 4;
      }
      else
      {
         %directDamage = %directDamage * 2;
      }
      %col.spawnExplosion(critProjectile, 1);
   }

   %col.setVelocity(vectorAdd(%col.getVelocity(), vectorAdd(vectorScale(%obj.getForwardVector(), 8), "0 0 3")));
   %col.damage(%obj, %pos, %directDamage, %damageType);
}

function ShortRifleImage::isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hits)
{
	if(%col.getType() & $TypeMasks::PlayerObjectType && %hits > 0)
	{
	%hitHead = getWord(%pos, 2) > getWord(%col.getWorldBoxCenter(), 2) - 3.3 * getWord(%col.getScale(), 2);
	%hitBottom = VectorDot(%normal, "0 0 -1") > 0;
	if(%hitBottom && %obj != %col)
	return 2;
	else if(%hitHead)
	return 1;
	}
}

function ShortRifle::doRaycast(%start, %aimVec, %range, %targets, %ignore)
{
   %ray = containerRayCast(%start, VectorAdd(%start, VectorScale(%aimVec, %range)), %targets, %ignore);
   if(isObject(getWord(%ray, 0)))
      return %ray SPC %range - VectorLen(VectorSub(posFromRaycast(%ray), %start));
   return "0" SPC VectorAdd(%start, VectorScale(%aimVec, %range));
}

function ShortRifle::checkForObstruction(%obj, %targets)
{
   %start = %obj.getEyePoint();
   %ray = containerRayCast(%start, vectorAdd(%start, vectorScale(%obj.getEyeVector(), 4.5)), %targets, %obj);
   return isObject(firstWord(%ray));
}
