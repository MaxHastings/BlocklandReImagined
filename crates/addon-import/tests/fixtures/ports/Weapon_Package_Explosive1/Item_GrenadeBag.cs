// A bag of grenades lying about.
datablock ItemData(GrenadeBagItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./bag.dts";
   uiName = "Stand-in Grenade Bag";
   canDrop = true;
   TT_ammoPickup = true;
   TT_ammoPickup[0] = "fragNades 1";
   TT_ammoPickup[1] = "molNades 1";
};

function GrenadeBagItem::onAdd(%this, %obj)
{
   %this.TT_initAmmoPickup(%obj);
   %obj.rotate = true;
   Parent::onAdd(%this, %obj);
}
