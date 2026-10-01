// Stand-in for the Tool_Duplicator port tests (CC0): a wand whose swing
// selects a stack, with the function shapes the port reads and its own
// numbers (reach 8, highlight 2 seconds). The port, not this script, is
// what runs.
datablock ExplosionData(DuplorcatorExplosion)
{
   lifetimeMS = 150;
};

datablock ProjectileData(DuplorcatorProjectile)
{
   explosion      = DuplorcatorExplosion;
   explodeOnDeath = true;
   lifetime       = 0;
   range          = 8;
};

datablock ItemData(DuplorcatorItem)
{
   category  = "Weapon";
   className = "Tool";
   shapeFile = "./wand.dts";
   uiName    = "Stand-in Duplicator";
   image     = DuplorcatorImage;
   canDrop   = true;
};

datablock ShapeBaseImageData(DuplorcatorImage)
{
   shapeFile  = "./wand.dts";
   mountPoint = 0;
   item       = DuplorcatorItem;
   projectile = DuplorcatorProjectile;
   armReady   = true;
   showBricks = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.1;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";
   stateAllowImageChange[1]        = true;

   stateName[2]                    = "Fire";
   stateTransitionOnTimeout[2]     = "Ready";
   stateTimeoutValue[2]            = 0.2;
   stateFire[2]                    = true;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;
};

function serverCmdDuplorcator(%client)
{
   if(isObject(%client.player))
      %client.player.mountImage(DuplorcatorImage, 0);
}

function serverCmdDup(%client)
{
   serverCmdDuplorcator(%client);
}

function DuplorcatorImage::onFire(%this, %player)
{
   %eye = %player.getEyePoint();
   %end = vectorAdd(%eye, vectorScale(%player.getEyeVector(), 8));
   %ray = containerRayCast(%eye, %end, $TypeMasks::FxBrickObjectType, %player);
   if(!isObject(%ray))
      return;
   if(getTrustLevel(firstWord(%ray), %player) < $Pref::Duplorcator::TrustLevel)
      return;
   %stack = %ray.getStack(%player);
   %player.tempBrick.stack = %stack;
}

function FxDtsBrick::getStack(%brick, %player)
{
   %stack = new SimSet();
   %stack.add(%brick);
   %brick.highlightColorReset = %brick.schedule(2000, restoreOrignalColors);
   for(%current = 0; %current < %stack.getCount(); %current++)
   {
      %brick = %stack.getObject(%current);
      if(%current)
      {
         for(%i = 0; %i < %brick.getNumDownBricks(); %i++)
            %stack.add(%brick.getDownBrick(%i));
      }
      for(%i = 0; %i < %brick.getNumUpBricks(); %i++)
         %stack.add(%brick.getUpBrick(%i));
   }
   return %stack;
}

package Duplorcator
{
   function serverCmdPlantBrick(%client)
   {
      %stack = %client.player.tempBrick.stack;
      if(!isObject(%stack))
         return Parent::serverCmdPlantBrick(%client);
      for(%i = 0; %i < %stack.numBricks; %i++)
         %planted += isObject(createFromStr(%stack.brick[%i], %client));
      commandToClient(%client, 'centerPrint', %planted @ " bricks duplicated successfully", 3);
   }

   function serverCmdUndoBrick(%client)
   {
      if(getField(%client.undoStack.val[%client.undoStack.head - 1], 1) $= "DUPLICATION")
         %client.undoStack.val[%client.undoStack.head - 1] = "";
      Parent::serverCmdUndoBrick(%client);
   }
};
activatePackage(Duplorcator);
