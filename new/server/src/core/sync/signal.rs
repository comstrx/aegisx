use tokio::sync::watch;

use super::arch::{Signal, Watch};

impl Signal {

    pub fn new () -> ( Self, Watch ) {

        let ( sender, receiver ) = watch::channel(false);

        ( Self { sender }, Watch { receiver } )

    }

    pub fn fire ( &self ) {

        let _ = self.sender.send(true);

    }

    pub fn fired ( &self ) -> bool {

        *self.sender.borrow()

    }

    pub fn watch ( &self ) -> Watch {

        Watch { receiver: self.sender.subscribe() }

    }

}

impl Watch {

    pub fn fired ( &self ) -> bool {

        *self.receiver.borrow()

    }

    pub async fn wait ( &mut self ) {

        let _ = self.receiver.wait_for(|fired| *fired).await;

    }

}
