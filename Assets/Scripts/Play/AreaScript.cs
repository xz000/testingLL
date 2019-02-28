using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Networking;
using FixMath;

public class AreaScript : MonoBehaviour {

    // Use this for initialization
    public int diameter;
    public int radius = 10;
    public int speed = 1;
    float waittime =10f;
    float gonetime = 0;

    // Use this for initialization
    void Start()
    {
        diameter = radius * 2;
        Vector3 startscale = new Vector3(diameter, diameter, 1);
        gameObject.transform.localScale = startscale;
    }

    private void FixedUpdate()
    {
        if (gonetime >= waittime)
        {
            radius -= speed;
            diameter = radius * 2;
            Vector3 startscale = new Vector3(diameter, diameter, 1);
            gameObject.transform.localScale = startscale;
            gonetime -= waittime;
            if (radius <= 0)
                enabled = false;
        }
        gonetime += Time.fixedDeltaTime;
    }
}
