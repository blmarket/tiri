See how this project handles event, cmd_find_state etc. Currently
"window-closed" event supposed to be fired when window being closed, but it's
burdensome to check the last drop per each Rc of window being dropped.
Preferably we should migrate them to Drop handler, but drop means we no longer
hold the Rc for the event notification. OTOH those hooks / event notifications
deeply dependent on existence of those strong ref.

Conceptually, events / notifications / hooks does not need to own the entities.
it should be able to run in borrowed instances. In reality, there are usages
which need long lifetime hold, which is not suitable with borrow entities, such
as alerts.

How we can resolve this and make all happy?

