library(ggplot2)
data <- read.csv("./metrics-histogram-fidelity/results.csv", header = FALSE)
ggplot(data, aes(x = V1, y = V2, color = V3)) + 
    labs(x = "quantile", y = "error %", color = "source") +
    geom_path() + 
    facet_wrap(~ V3) +
    ggsave("tmp.png", width = 10, height = 5)