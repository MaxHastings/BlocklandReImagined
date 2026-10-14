Players not riding a vehicle get its pose as `Datagram::VehicleView` ("w"):
the drawing fields only, without the crew's prediction state. Its crew still
get the full `Datagram::Vehicle`.
