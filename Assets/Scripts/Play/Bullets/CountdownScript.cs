using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class CountdownScript : MonoBehaviour
{
    public float maxtime = 3;
    float currenttime = 0;

    private void FixedUpdate()
    {
        currenttime += Time.fixedDeltaTime;
        if (currenttime >= maxtime)
            GetComponent<DestroyScript>().Destroyself();
    }
}
