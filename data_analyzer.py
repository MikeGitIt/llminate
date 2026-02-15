#!/usr/bin/env python3

class DataAnalyzer:
    """
    A class for analyzing and processing data.
    
    Attributes:
        data (list): The data to analyze
        name (str): Name of the data set
    """
    
    def __init__(self, data, name="Default Dataset"):
        """
        Initialize the DataAnalyzer with data.
        
        Args:
            data (list): The data to analyze
            name (str, optional): Name of the dataset. Defaults to "Default Dataset".
        """
        self.data = data
        self.name = name
        self.processed = False
        self._stats = None
    
    def process(self):
        """Process the data and calculate statistics."""
        if not self.data:
            print("No data to process")
            return False
        
        # Calculate basic statistics
        self._stats = {
            "count": len(self.data),
            "sum": sum(self.data),
            "mean": sum(self.data) / len(self.data) if self.data else None,
            "min": min(self.data) if self.data else None,
            "max": max(self.data) if self.data else None
        }
        
        # Calculate median
        sorted_data = sorted(self.data)
        n = len(sorted_data)
        if n % 2 == 0:
            self._stats["median"] = (sorted_data[n//2 - 1] + sorted_data[n//2]) / 2
        else:
            self._stats["median"] = sorted_data[n//2]
        
        self.processed = True
        return True
    
    def get_statistics(self):
        """
        Get the calculated statistics.
        
        Returns:
            dict: Dictionary containing statistics or None if not processed
        """
        if not self.processed:
            print("Data has not been processed yet. Call process() first.")
            return None
        return self._stats
    
    def add_data(self, new_data):
        """
        Add new data to the existing dataset.
        
        Args:
            new_data (list): New data to add
            
        Returns:
            bool: True if successful, False otherwise
        """
        if not isinstance(new_data, list):
            print("New data must be a list")
            return False
        
        self.data.extend(new_data)
        self.processed = False  # Reset processed flag
        return True
    
    def __str__(self):
        """String representation of the DataAnalyzer."""
        return f"DataAnalyzer(name='{self.name}', count={len(self.data)}, processed={self.processed})"


def main():
    """Main function to demonstrate the DataAnalyzer class."""
    # Sample data
    sample_data = [12, 15, 23, 45, 67, 89, 21, 34]
    
    # Create an analyzer
    analyzer = DataAnalyzer(sample_data, "Sample Dataset")
    print(f"Created: {analyzer}")
    
    # Process data
    analyzer.process()
    
    # Get statistics
    stats = analyzer.get_statistics()
    if stats:
        print("\nStatistics:")
        for key, value in stats.items():
            print(f"  {key}: {value}")
    
    # Add more data
    analyzer.add_data([100, 200, 300])
    print(f"\nAfter adding data: {analyzer}")
    
    # Process again
    analyzer.process()
    
    # Get updated statistics
    stats = analyzer.get_statistics()
    if stats:
        print("\nUpdated Statistics:")
        for key, value in stats.items():
            print(f"  {key}: {value}")


if __name__ == "__main__":
    main()