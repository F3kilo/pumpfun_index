
var chart = new Chart(document.getElementById("myChart"), {
  type: 'line',
  data: {
    labels: [],
    datasets: [{ 
        data: [],
        label: "BTC",
        borderColor: "#3e95cd",
        fill: false
      } 
    ]
  },
  options: {
    title: {
      display: true,
      text: 'Prices'
    }
  }
});

var socket = new WebSocket("ws://localhost:33987/price_ws");

socket.onmessage = function(event) {
  var data = JSON.parse(event.data);
  var date = new Date(data.bitcoin.last_updated_at * 1000);

  var options = {
    hour: 'numeric',
    minute: 'numeric',
    second: 'numeric'
  };

  var data_str = date.toLocaleString("en-US", options);

  chart.data.labels.push(data_str);
  chart.data.datasets.forEach((dataset) => {
    dataset.data.push(data.bitcoin.usd);
  });

  if (chart.data.labels.length > 100) {
    chart.data.labels.shift();
    chart.data.datasets.forEach((dataset) => {
      dataset.data.shift();
    });
  }

  chart.update();
};
