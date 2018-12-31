using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class RBScript : MonoBehaviour
{
    Rigidbody2D PlayerRB2D;
    MoveScript MS;
    float timetostop;
    float timepassed;

	// Use this for initialization
	void Start () {
        PlayerRB2D = GetComponent<Rigidbody2D>();
        MS = GetComponent<MoveScript>();
    }

    public void GetKicked(Vector2 force)
    {
        PlayerRB2D.AddForce(force);
    }

    public void GetPushed(Fix64Vector2 velo,float time)
    {
        MS.controllable = false;
        MS.Givenvelocity = velo;
        timepassed = 0;
        timetostop = time;
    }
    
    private void FixedUpdate()
    {
        if (MS.controllable)
            return;
        if (timepassed >= timetostop)
        {
            MS.Givenvelocity = Fix64Vector2.Zero;
            MS.controllable = true;
        }
        else
            timepassed += Time.fixedDeltaTime;
    }

    public float GetRemainTime()
    {
        return timetostop - timepassed;
    }

    public void PushEnd()
    {
        MS.Givenvelocity = Fix64Vector2.Zero;
        MS.controllable = true;
        //timepassed = timetostop;
    }
}
