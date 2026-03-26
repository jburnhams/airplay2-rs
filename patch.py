with open("src/client/mod.rs", "r") as f:
    content = f.read()

replacement = """    pub async fn seek(&self, position: Duration) -> Result<(), AirPlayError> {
        self.ensure_connected().await?;
        self.playback.seek(position).await
    }

    /// Fast forward
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn fast_forward(&self) -> Result<(), AirPlayError> {
        self.ensure_connected().await?;
        self.playback.fast_forward().await
    }

    /// Rewind
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn rewind(&self) -> Result<(), AirPlayError> {
        self.ensure_connected().await?;
        self.playback.rewind().await
    }

    /// Get current playback state"""

content = content.replace("""    pub async fn seek(&self, position: Duration) -> Result<(), AirPlayError> {
        self.ensure_connected().await?;
        self.playback.seek(position).await
    }

    /// Get current playback state""", replacement)

with open("src/client/mod.rs", "w") as f:
    f.write(content)
