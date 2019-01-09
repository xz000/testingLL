using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class FakeCircleScript : MonoBehaviour
{
    public Rigidbody2D Beauty;
    public Rigidbody2D selfRB;
    //Vector2 v;
    public float maxtime = 3;
    float currenttime = 0;

	// Use this for initialization
	/*void Start ()
    {
        //v = selfRB.position - Beauty.position;
        GetComponent<HPScript>().currentHP = Beauty.gameObject.GetComponent<HPScript>().currentHP;
    }*/

    private void FixedUpdate()
    {
        currenttime += Time.fixedDeltaTime;
        if (currenttime >= maxtime)
            GetComponent<DestroyScript>().Destroyself();
        selfRB.velocity = Beauty.velocity;
    }
}
